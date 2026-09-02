use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use dashmap::DashMap;
use parking_lot::Mutex as ParkingMutex;
use russh::client;
use russh::{Channel, ChannelMsg, Disconnect};
use russh_sftp::client::SftpSession as RusshSftpSession;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener as TokioTcpListener;
use tokio::net::TcpStream as TokioTcpStream;
use tokio::sync::{mpsc, Mutex};
use uuid::Uuid;

use crate::models::{
    ConnectParams, PortForwardRule, SessionStatusEvent, SftpEntry, TransferProgress,
};
use crate::ssh::client::{
    authenticate_with_key, authenticate_with_password, decode_channel_data, load_private_key,
    SshClientHandler,
};

struct SessionInner {
    handle: Arc<Mutex<client::Handle<SshClientHandler>>>,
    write_tx: mpsc::UnboundedSender<Vec<u8>>,
    resize_tx: mpsc::UnboundedSender<(u32, u32)>,
    sftp: Mutex<Option<RusshSftpSession>>,
    port_forwards: ParkingMutex<Vec<PortForwardHandle>>,
    cancel: tokio::sync::watch::Sender<bool>,
}

struct PortForwardHandle {
    rule: PortForwardRule,
    task: tokio::task::JoinHandle<()>,
}

pub struct SessionManager {
    sessions: DashMap<String, Arc<SessionInner>>,
    app: AppHandle,
}

impl SessionManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            sessions: DashMap::new(),
            app,
        }
    }

    pub async fn connect(&self, params: ConnectParams) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();

        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(3600)),
            ..Default::default()
        });

        let addr = format!("{}:{}", params.host, params.port);
        let mut handle = client::connect(config, &addr, SshClientHandler)
            .await
            .context("failed to connect to SSH server")?;

        if params.auth_type == "key" {
            let key_path = params
                .key_path
                .as_deref()
                .ok_or_else(|| anyhow!("key path required"))?;
            let key = load_private_key(key_path, params.key_passphrase.as_deref())?;
            authenticate_with_key(&mut handle, &params.username, key).await?;
        } else {
            authenticate_with_password(
                &mut handle,
                &params.username,
                params.password.as_deref().unwrap_or(""),
            )
            .await?;
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

        let inner = Arc::new(SessionInner {
            handle: handle.clone(),
            write_tx,
            resize_tx,
            sftp: Mutex::new(None),
            port_forwards: ParkingMutex::new(Vec::new()),
            cancel: cancel_tx,
        });

        self.sessions.insert(session_id.clone(), inner.clone());

        let app = self.app.clone();
        let sid = session_id.clone();
        tokio::spawn(async move {
            shell_loop(app, sid, channel, write_rx, resize_rx, cancel_rx).await;
        });

        Ok(session_id)
    }

    pub async fn disconnect(&self, session_id: &str) -> Result<()> {
        if let Some((_, inner)) = self.sessions.remove(session_id) {
            let _ = inner.cancel.send(true);
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

    async fn ensure_sftp(&self, session_id: &str) -> Result<Arc<SessionInner>> {
        let inner = self
            .sessions
            .get(session_id)
            .map(|e| e.clone())
            .ok_or_else(|| anyhow!("session not found"))?;

        if inner.sftp.lock().await.is_some() {
            return Ok(inner);
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

        let sftp = RusshSftpSession::new(channel.into_stream())
            .await
            .context("failed to init SFTP")?;

        *inner.sftp.lock().await = Some(sftp);
        Ok(inner)
    }

    pub async fn list_sftp_dir(&self, session_id: &str, path: &str) -> Result<Vec<SftpEntry>> {
        let inner = self.ensure_sftp(session_id).await?;
        let sftp = inner.sftp.lock().await;
        let sftp = sftp.as_ref().ok_or_else(|| anyhow!("SFTP unavailable"))?;
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

    pub async fn download_file(
        &self,
        session_id: &str,
        remote_path: &str,
        local_path: &str,
    ) -> Result<String> {
        let transfer_id = Uuid::new_v4().to_string();
        let file_name = remote_path
            .rsplit('/')
            .next()
            .unwrap_or("file")
            .to_string();

        let inner = self.ensure_sftp(session_id).await?;
        let sftp = inner.sftp.lock().await;
        let sftp = sftp.as_ref().ok_or_else(|| anyhow!("SFTP unavailable"))?;

        let metadata = sftp
            .metadata(remote_path)
            .await
            .context("failed to stat remote file")?;
        let total = metadata.size.unwrap_or(0);

        self.emit_transfer(TransferProgress {
            transfer_id: transfer_id.clone(),
            session_id: session_id.to_string(),
            file_name: file_name.clone(),
            direction: "download".to_string(),
            bytes_transferred: 0,
            total_bytes: total,
            status: "in_progress".to_string(),
            error: None,
        });

        let mut remote = sftp
            .open(remote_path)
            .await
            .context("failed to open remote file")?;
        let mut local = tokio::fs::File::create(local_path)
            .await
            .context("failed to create local file")?;

        let mut transferred = 0u64;
        let mut buf = vec![0u8; 32768];
        loop {
            let n = remote.read(&mut buf).await.context("read error")?;
            if n == 0 {
                break;
            }
            local.write_all(&buf[..n]).await.context("write error")?;
            transferred += n as u64;
            self.emit_transfer(TransferProgress {
                transfer_id: transfer_id.clone(),
                session_id: session_id.to_string(),
                file_name: file_name.clone(),
                direction: "download".to_string(),
                bytes_transferred: transferred,
                total_bytes: total,
                status: "in_progress".to_string(),
                error: None,
            });
        }

        self.emit_transfer(TransferProgress {
            transfer_id,
            session_id: session_id.to_string(),
            file_name,
            direction: "download".to_string(),
            bytes_transferred: transferred,
            total_bytes: total,
            status: "completed".to_string(),
            error: None,
        });

        Ok(local_path.to_string())
    }

    pub async fn upload_file(
        &self,
        session_id: &str,
        local_path: &str,
        remote_path: &str,
    ) -> Result<String> {
        let transfer_id = Uuid::new_v4().to_string();
        let file_name = local_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("file")
            .to_string();

        let meta = tokio::fs::metadata(local_path)
            .await
            .context("failed to stat local file")?;
        let total = meta.len();

        self.emit_transfer(TransferProgress {
            transfer_id: transfer_id.clone(),
            session_id: session_id.to_string(),
            file_name: file_name.clone(),
            direction: "upload".to_string(),
            bytes_transferred: 0,
            total_bytes: total,
            status: "in_progress".to_string(),
            error: None,
        });

        let inner = self.ensure_sftp(session_id).await?;
        let sftp = inner.sftp.lock().await;
        let sftp = sftp.as_ref().ok_or_else(|| anyhow!("SFTP unavailable"))?;

        let mut local = tokio::fs::File::open(local_path)
            .await
            .context("failed to open local file")?;
        let mut remote = sftp
            .create(remote_path)
            .await
            .context("failed to create remote file")?;

        let mut transferred = 0u64;
        let mut buf = vec![0u8; 32768];
        loop {
            let n = local.read(&mut buf).await.context("read error")?;
            if n == 0 {
                break;
            }
            remote.write_all(&buf[..n]).await.context("write error")?;
            transferred += n as u64;
            self.emit_transfer(TransferProgress {
                transfer_id: transfer_id.clone(),
                session_id: session_id.to_string(),
                file_name: file_name.clone(),
                direction: "upload".to_string(),
                bytes_transferred: transferred,
                total_bytes: total,
                status: "in_progress".to_string(),
                error: None,
            });
        }

        self.emit_transfer(TransferProgress {
            transfer_id,
            session_id: session_id.to_string(),
            file_name,
            direction: "upload".to_string(),
            bytes_transferred: transferred,
            total_bytes: total,
            status: "completed".to_string(),
            error: None,
        });

        Ok(remote_path.to_string())
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

    fn emit_transfer(&self, progress: TransferProgress) {
        let _ = self.app.emit("sftp-transfer-progress", &progress);
    }
}

async fn shell_loop(
    app: AppHandle,
    session_id: String,
    mut channel: Channel<client::Msg>,
    mut write_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut resize_rx: mpsc::UnboundedReceiver<(u32, u32)>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        if *cancel.borrow() {
            break;
        }

        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    break;
                }
            }
            Some(data) = write_rx.recv() => {
                if channel.data(&data[..]).await.is_err() {
                    break;
                }
            }
            Some((cols, rows)) = resize_rx.recv() => {
                let _ = channel.window_change(cols, rows, 0, 0).await;
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        let text = String::from_utf8_lossy(&data).to_string();
                        let _ = app.emit(&format!("terminal-output:{}", session_id), text);
                    }
                    Some(ChannelMsg::ExitStatus { .. }) | Some(ChannelMsg::Eof) | None => {
                        let _ = app.emit(
                            &format!("session-status:{}", session_id),
                            SessionStatusEvent {
                                status: "disconnected".to_string(),
                                error: None,
                            },
                        );
                        break;
                    }
                    Some(other) => {
                        if let Some(data) = decode_channel_data(other) {
                            let text = String::from_utf8_lossy(&data).to_string();
                            let _ = app.emit(&format!("terminal-output:{}", session_id), text);
                        }
                    }
                }
            }
        }
    }

    let _ = channel.eof().await;
    let _ = channel.close().await;
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
        let mut handle = handle.lock().await;
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
