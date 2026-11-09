use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use dashmap::DashMap;
use parking_lot::Mutex as ParkingMutex;
use russh::client;
use russh::keys::PublicKey;
use russh::{Channel, ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession as RusshSftpSession;
use tauri::{AppHandle, Emitter};
use tokio::net::TcpListener as TokioTcpListener;
use tokio::net::TcpStream as TokioTcpStream;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::history::{EndReason, History, SessionLog, Utf8Stream};
use crate::models::{
    ConnectError, ConnectParams, PortForwardRule, ResolvedAuth, SessionStatusEvent, SftpEntry,
    TransferProgress,
};
use crate::ssh::client::{
    authenticate_with_key, authenticate_with_password, decode_channel_data, load_private_key,
    HostKeyRejection, SshClientHandler,
};
use crate::ssh::known_hosts::KnownHosts;
use crate::ssh::transfer::{self, base_name, Transfer};

struct SessionInner {
    handle: Arc<Mutex<client::Handle<SshClientHandler>>>,
    write_tx: mpsc::UnboundedSender<Vec<u8>>,
    resize_tx: mpsc::UnboundedSender<(u32, u32)>,
    sftp: Mutex<Option<Arc<RusshSftpSession>>>,
    port_forwards: ParkingMutex<Vec<PortForwardHandle>>,
    cancel: tokio::sync::watch::Sender<bool>,
    log: Arc<SessionLog>,
}

struct PortForwardHandle {
    rule: PortForwardRule,
    task: tokio::task::JoinHandle<()>,
}

pub struct SessionManager {
    sessions: DashMap<String, Arc<SessionInner>>,
    known_hosts: Arc<KnownHosts>,
    history: Arc<History>,
    app: AppHandle,
}

/// Payload of `terminal-output:<session id>` events.
#[derive(Clone, serde::Serialize)]
pub struct TerminalOutput {
    /// Stream position after this chunk (see [`SessionManager::scrollback`]).
    pub seq: u64,
    pub data: String,
}

/// Recent output of a session and the stream position it ends at.
#[derive(serde::Serialize)]
pub struct ScrollbackSnapshot {
    pub seq: u64,
    pub data: String,
}

impl SessionManager {
    pub fn new(app: AppHandle, known_hosts_path: PathBuf, history: Arc<History>) -> Self {
        Self {
            sessions: DashMap::new(),
            known_hosts: Arc::new(KnownHosts::new(known_hosts_path)),
            history,
            app,
        }
    }

    /// Recent output of a live (or ended but not yet closed) session.
    pub fn scrollback(&self, session_id: &str) -> Result<ScrollbackSnapshot> {
        let inner = self
            .sessions
            .get(session_id)
            .ok_or_else(|| anyhow!("session not found"))?;
        let (data, seq) = inner.log.scrollback.snapshot();
        Ok(ScrollbackSnapshot { seq, data })
    }

    /// Save the output of all open sessions; called when the app exits.
    pub fn save_all_history(&self) {
        for entry in self.sessions.iter() {
            entry.log.save(EndReason::AppExit);
        }
    }

    /// Record a host key the user confirmed in the UI.
    pub fn trust_host_key(&self, host: &str, port: u16, key: &str, replace: bool) -> Result<()> {
        let key = PublicKey::from_openssh(key).context("invalid host key")?;
        self.known_hosts.trust(host, port, &key, replace)
    }

    pub async fn connect(&self, params: ConnectParams) -> Result<String, ConnectError> {
        let session_id = Uuid::new_v4().to_string();

        // Send keepalives so idle sessions aren't dropped by NAT/firewalls or
        // server idle timers; treat the server as dead after 3 missed replies.
        let config = Arc::new(client::Config {
            inactivity_timeout: None,
            keepalive_interval: Some(Duration::from_secs(30)),
            keepalive_max: 3,
            ..Default::default()
        });

        let rejection = Arc::new(ParkingMutex::new(None));
        let handler = SshClientHandler::new(
            params.host.clone(),
            params.port,
            self.known_hosts.clone(),
            rejection.clone(),
        );
        let addr = (params.host.as_str(), params.port);
        let mut handle = match client::connect(config, addr, handler).await {
            Ok(handle) => handle,
            Err(e) => {
                return Err(match rejection.lock().take() {
                    Some(HostKeyRejection::Unknown(info)) => ConnectError::UnknownHostKey(info),
                    Some(HostKeyRejection::Changed(info)) => ConnectError::HostKeyChanged(info),
                    None => e.context("failed to connect to SSH server").into(),
                });
            }
        };

        match &params.auth {
            ResolvedAuth::Key { path, passphrase } => {
                let key = load_private_key(path, passphrase.as_deref())?;
                authenticate_with_key(&mut handle, &params.username, key).await?;
            }
            ResolvedAuth::Password(password) => {
                authenticate_with_password(&mut handle, &params.username, password).await?;
            }
        }

        let channel = handle
            .channel_open_session()
            .await
            .context("failed to open session channel")?;

        channel
            .request_pty(false, "xterm-256color", 80, 24, 0, 0, &[])
            .await
            .context("failed to request PTY")?;

        channel
            .request_shell(false)
            .await
            .context("failed to request shell")?;

        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let (write_tx, write_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (resize_tx, resize_rx) = mpsc::unbounded_channel::<(u32, u32)>();

        let handle = Arc::new(Mutex::new(handle));
        let log = Arc::new(SessionLog::new(
            self.history.clone(),
            params.profile_id.clone(),
            params.host.clone(),
            params.port,
            params.username.clone(),
        ));

        let inner = Arc::new(SessionInner {
            handle: handle.clone(),
            write_tx,
            resize_tx,
            sftp: Mutex::new(None),
            port_forwards: ParkingMutex::new(Vec::new()),
            cancel: cancel_tx,
            log: log.clone(),
        });

        self.sessions.insert(session_id.clone(), inner.clone());

        let app = self.app.clone();
        let sid = session_id.clone();
        tokio::spawn(async move {
            let reason = shell_loop(&app, &sid, &log, channel, write_rx, resize_rx, cancel_rx).await;
            let _ = tokio::task::spawn_blocking(move || log.save(reason)).await;
        });

        Ok(session_id)
    }

    pub async fn disconnect(&self, session_id: &str) -> Result<()> {
        if let Some((_, inner)) = self.sessions.remove(session_id) {
            let _ = inner.cancel.send(true);
            // Save now rather than when the shell task notices the cancel,
            // so reconnecting right away already shows this session.
            let log = inner.log.clone();
            let _ = tokio::task::spawn_blocking(move || log.save(EndReason::Closed)).await;
            for pf in inner.port_forwards.lock().drain(..) {
                pf.task.abort();
            }
            let handle = inner.handle.lock().await;
            let _ = handle
                .disconnect(Disconnect::ByApplication, "closed by user", "en")
                .await;
        }
        Ok(())
    }

    pub async fn write(&self, session_id: &str, data: &str) -> Result<()> {
        let inner = self
            .sessions
            .get(session_id)
            .ok_or_else(|| anyhow!("session not found"))?;
        inner
            .write_tx
            .send(data.as_bytes().to_vec())
            .map_err(|_| anyhow!("session channel closed"))?;
        Ok(())
    }

    pub async fn resize(&self, session_id: &str, cols: u32, rows: u32) -> Result<()> {
        let inner = self
            .sessions
            .get(session_id)
            .ok_or_else(|| anyhow!("session not found"))?;
        inner
            .resize_tx
            .send((cols, rows))
            .map_err(|_| anyhow!("session channel closed"))?;
        Ok(())
    }

    /// The session's SFTP client, opened on first use. Requests on it can
    /// run concurrently, so browsing works while a transfer is running.
    async fn sftp(&self, session_id: &str) -> Result<Arc<RusshSftpSession>> {
        let inner = self
            .sessions
            .get(session_id)
            .map(|e| e.clone())
            .ok_or_else(|| anyhow!("session not found"))?;

        let mut slot = inner.sftp.lock().await;
        if let Some(sftp) = slot.as_ref() {
            return Ok(sftp.clone());
        }

        let channel = {
            let handle = inner.handle.lock().await;
            handle
                .channel_open_session()
                .await
                .context("failed to open SFTP channel")?
        };

        channel
            .request_subsystem(true, "sftp")
            .await
            .context("failed to request SFTP subsystem")?;

        let sftp = Arc::new(
            RusshSftpSession::new(channel.into_stream())
                .await
                .context("failed to init SFTP")?,
        );
        *slot = Some(sftp.clone());
        Ok(sftp)
    }

    pub async fn list_sftp_dir(&self, session_id: &str, path: &str) -> Result<Vec<SftpEntry>> {
        let sftp = self.sftp(session_id).await?;
        let read_dir = sftp.read_dir(path).await.context("failed to read dir")?;

        let mut entries = Vec::new();
        for entry in read_dir {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let attrs = entry.metadata();
            entries.push(SftpEntry {
                name,
                path: entry.path(),
                is_dir: attrs.is_dir(),
                size: attrs.size.unwrap_or(0),
                modified: attrs.mtime.unwrap_or(0) as u64,
            });
        }

        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        Ok(entries)
    }

    fn progress_emitter(&self) -> impl Fn(&TransferProgress) + Send + Sync + 'static {
        let app = self.app.clone();
        move |p| {
            let _ = app.emit("sftp-transfer-progress", p);
        }
    }

    /// Download a remote file to `local_path`.
    pub async fn download_file(
        &self,
        session_id: &str,
        remote_path: &str,
        local_path: &str,
    ) -> Result<String> {
        let mut t = Transfer::start(self.progress_emitter(), session_id, base_name(remote_path), "download");
        let result = async {
            let sftp = self.sftp(session_id).await?;
            transfer::download_file(&sftp, remote_path, Path::new(local_path), &mut t).await
        }
        .await;
        t.finish(result)?;
        Ok(local_path.to_string())
    }

    /// Download a remote folder recursively into the local folder `local_dir`.
    /// Returns the path of the created folder.
    pub async fn download_dir(
        &self,
        session_id: &str,
        remote_path: &str,
        local_dir: &str,
    ) -> Result<String> {
        let mut t = Transfer::start(self.progress_emitter(), session_id, base_name(remote_path), "download");
        let result = async {
            let sftp = self.sftp(session_id).await?;
            transfer::download_dir(&sftp, remote_path, Path::new(local_dir), &mut t).await
        }
        .await;
        let root = t.finish(result)?;
        Ok(root.to_string_lossy().into_owned())
    }

    /// Delete a remote file or folder (recursively).
    pub async fn delete_path(&self, session_id: &str, path: &str) -> Result<usize> {
        let sftp = self.sftp(session_id).await?;
        transfer::delete(&sftp, path).await
    }

    /// Upload local files and folders into `remote_dir`, one transfer each.
    /// Returns the first error after trying all of them.
    pub async fn upload_paths(
        &self,
        session_id: &str,
        local_paths: &[String],
        remote_dir: &str,
    ) -> Result<()> {
        let mut first_err = None;
        for local in local_paths {
            let mut t = Transfer::start(self.progress_emitter(), session_id, base_name(local), "upload");
            let result = async {
                let sftp = self.sftp(session_id).await?;
                transfer::upload(&sftp, Path::new(local), remote_dir, &mut t).await
            }
            .await;
            if let Err(e) = t.finish(result) {
                first_err.get_or_insert(e);
            }
        }
        first_err.map_or(Ok(()), Err)
    }

    pub async fn add_port_forward(
        &self,
        session_id: &str,
        forward_type: &str,
        bind_host: &str,
        bind_port: u16,
        target_host: &str,
        target_port: u16,
    ) -> Result<PortForwardRule> {
        let inner = self
            .sessions
            .get(session_id)
            .map(|e| e.clone())
            .ok_or_else(|| anyhow!("session not found"))?;

        let rule = PortForwardRule {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            forward_type: forward_type.to_string(),
            bind_host: bind_host.to_string(),
            bind_port,
            target_host: target_host.to_string(),
            target_port,
        };

        let handle = inner.handle.clone();
        let rule_clone = rule.clone();
        let mut cancel_rx = inner.cancel.subscribe();

        let task = match forward_type {
            "local" => {
                let bind_addr = format!("{}:{}", bind_host, bind_port);
                let target_host = target_host.to_string();
                tokio::spawn(async move {
                    if let Err(e) = run_local_forward(
                        handle,
                        bind_addr,
                        target_host,
                        target_port,
                        &mut cancel_rx,
                    )
                    .await
                    {
                        tracing::error!("local forward error: {e}");
                    }
                })
            }
            "remote" => {
                let bind_host = bind_host.to_string();
                tokio::spawn(async move {
                    if let Err(e) =
                        run_remote_forward(handle, bind_host, bind_port, &mut cancel_rx).await
                    {
                        tracing::error!("remote forward error: {e}");
                    }
                })
            }
            other => return Err(anyhow!("unknown forward type: {other}")),
        };

        inner.port_forwards.lock().push(PortForwardHandle {
            rule: rule_clone,
            task,
        });

        Ok(rule)
    }

    pub fn list_port_forwards(&self, session_id: &str) -> Vec<PortForwardRule> {
        self.sessions
            .get(session_id)
            .map(|inner| {
                inner
                    .port_forwards
                    .lock()
                    .iter()
                    .map(|pf| pf.rule.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn remove_port_forward(&self, session_id: &str, rule_id: &str) -> Result<()> {
        if let Some(inner) = self.sessions.get(session_id) {
            let mut forwards = inner.port_forwards.lock();
            if let Some(idx) = forwards.iter().position(|pf| pf.rule.id == rule_id) {
                forwards.remove(idx).task.abort();
            }
        }
        Ok(())
    }
}

/// Pump terminal I/O until the session ends; returns why it ended.
async fn shell_loop(
    app: &AppHandle,
    session_id: &str,
    log: &SessionLog,
    mut channel: Channel<client::Msg>,
    mut write_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut resize_rx: mpsc::UnboundedReceiver<(u32, u32)>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) -> EndReason {
    let output_event = format!("terminal-output:{session_id}");
    let mut decoder = Utf8Stream::default();
    let emit_output = |decoder: &mut Utf8Stream, data: &[u8]| {
        let text = decoder.decode(data);
        if text.is_empty() {
            return;
        }
        let seq = log.scrollback.push(&text);
        let _ = app.emit(&output_event, TerminalOutput { seq, data: text });
    };

    let reason = loop {
        if *cancel.borrow() {
            break EndReason::Closed;
        }

        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    break EndReason::Closed;
                }
            }
            Some(data) = write_rx.recv() => {
                if channel.data(&data[..]).await.is_err() {
                    break EndReason::Disconnected;
                }
            }
            Some((cols, rows)) = resize_rx.recv() => {
                let _ = channel.window_change(cols, rows, 0, 0).await;
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => emit_output(&mut decoder, &data),
                    Some(ChannelMsg::ExitStatus { .. }) | Some(ChannelMsg::Eof) | None => {
                        break EndReason::Disconnected;
                    }
                    Some(other) => {
                        if let Some(data) = decode_channel_data(other) {
                            emit_output(&mut decoder, &data);
                        }
                    }
                }
            }
        }
    };

    if reason == EndReason::Disconnected {
        let _ = app.emit(
            &format!("session-status:{session_id}"),
            SessionStatusEvent {
                status: "disconnected".to_string(),
                error: None,
            },
        );
    }
    let _ = channel.eof().await;
    let _ = channel.close().await;
    reason
}

async fn run_local_forward(
    handle: Arc<Mutex<client::Handle<SshClientHandler>>>,
    bind_addr: String,
    target_host: String,
    target_port: u16,
    cancel: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let std_listener = TcpListener::bind(&bind_addr).context("failed to bind local port")?;
    std_listener.set_nonblocking(true)?;
    let listener = TokioTcpListener::from_std(std_listener)?;

    loop {
        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted.context("accept failed")?;
                let handle = handle.clone();
                let target_host = target_host.clone();
                tokio::spawn(async move {
                    if let Err(e) = pipe_local_connection(handle, stream, target_host, target_port).await {
                        tracing::debug!("forward connection closed: {e}");
                    }
                });
            }
        }
    }

    Ok(())
}

async fn pipe_local_connection(
    handle: Arc<Mutex<client::Handle<SshClientHandler>>>,
    mut local: TokioTcpStream,
    target_host: String,
    target_port: u16,
) -> Result<()> {
    let channel = {
        let handle = handle.lock().await;
        handle
            .channel_open_direct_tcpip(&target_host, target_port as u32, "127.0.0.1", 0)
            .await
            .context("failed to open direct-tcpip channel")?
    };

    let mut remote = channel.into_stream();
    tokio::io::copy_bidirectional(&mut local, &mut remote)
        .await
        .context("forward pipe failed")?;
    Ok(())
}

async fn run_remote_forward(
    handle: Arc<Mutex<client::Handle<SshClientHandler>>>,
    bind_host: String,
    bind_port: u16,
    cancel: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    {
        let handle = handle.lock().await;
        handle
            .tcpip_forward(&bind_host, bind_port as u32)
            .await
            .context("failed to request remote forward")?;
    }

    loop {
        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(3600)) => {}
        }
    }

    let handle = handle.lock().await;
    let _ = handle.cancel_tcpip_forward(&bind_host, bind_port as u32).await;
    Ok(())
}
