mod fsutil;
mod history;
mod models;
mod profiles;
mod ssh;

use std::sync::Arc;

use anyhow::anyhow;
use history::{History, HistoryEntry};
use models::{ConnectError, ConnectionProfile, PortForwardRule, SftpEntry};
use profiles::ProfileStore;
use ssh::{ScrollbackSnapshot, SessionManager};
use tauri::Manager;

/// Run blocking work (profile file + OS keychain, which may show an unlock
/// prompt) on a plain thread, outside both the UI thread and the Tokio
/// runtime. The Linux Secret Service client runs its own executor and must
/// not be nested inside Tokio.
async fn off_thread<T, F>(f: F) -> anyhow::Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.await.map_err(|_| anyhow!("background task failed"))?
}

#[tauri::command]
async fn list_profiles(
    state: tauri::State<'_, Arc<ProfileStore>>,
) -> Result<Vec<ConnectionProfile>, String> {
    let store = state.inner().clone();
    off_thread(move || store.list()).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn save_profile(
    state: tauri::State<'_, Arc<ProfileStore>>,
    profile: ConnectionProfile,
) -> Result<(), String> {
    let store = state.inner().clone();
    off_thread(move || store.save(profile)).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn delete_profile(
    state: tauri::State<'_, Arc<ProfileStore>>,
    history: tauri::State<'_, Arc<History>>,
    id: String,
) -> Result<(), String> {
    let store = state.inner().clone();
    let history = history.inner().clone();
    off_thread(move || {
        store.delete(&id)?;
        if let Err(e) = history.delete_profile(&id) {
            tracing::warn!("failed to delete session history: {e:#}");
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("{e:#}"))
}

/// Whether passwords are stored in the OS keychain (true) or fall back to
/// an owner-only file (false).
#[tauri::command]
async fn keychain_available(state: tauri::State<'_, Arc<ProfileStore>>) -> Result<bool, String> {
    let store = state.inner().clone();
    off_thread(move || Ok(store.keychain_available())).await.map_err(|e| format!("{e:#}"))
}

/// Connect using a saved profile. Host, credentials and secrets are read
/// from the profile store, so the frontend never handles saved secrets.
#[tauri::command]
async fn connect_ssh(
    profiles: tauri::State<'_, Arc<ProfileStore>>,
    sessions: tauri::State<'_, Arc<SessionManager>>,
    profile_id: String,
) -> Result<String, ConnectError> {
    let store = profiles.inner().clone();
    let params = off_thread(move || store.resolve(&profile_id)).await?;
    sessions.connect(params).await
}

#[tauri::command]
fn trust_host_key(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    host: String,
    port: u16,
    key: String,
    replace: bool,
) -> Result<(), String> {
    sessions
        .trust_host_key(&host, port, &key, replace)
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn disconnect_ssh(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<(), String> {
    sessions.disconnect(&session_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn write_terminal(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    sessions
        .write(&session_id, &data)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn resize_terminal(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    sessions
        .resize(&session_id, cols, rows)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_sftp_dir(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    session_id: String,
    path: String,
) -> Result<Vec<SftpEntry>, String> {
    sessions
        .list_sftp_dir(&session_id, &path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn download_sftp_file(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    session_id: String,
    remote_path: String,
    local_path: String,
) -> Result<String, String> {
    sessions
        .download_file(&session_id, &remote_path, &local_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn upload_sftp_file(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    session_id: String,
    local_path: String,
    remote_path: String,
) -> Result<String, String> {
    sessions
        .upload_file(&session_id, &local_path, &remote_path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_port_forwards(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<Vec<PortForwardRule>, String> {
    Ok(sessions.list_port_forwards(&session_id))
}

#[tauri::command]
async fn add_port_forward(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    session_id: String,
    forward_type: String,
    bind_host: String,
    bind_port: u16,
    target_host: String,
    target_port: u16,
) -> Result<PortForwardRule, String> {
    sessions
        .add_port_forward(
            &session_id,
            &forward_type,
            &bind_host,
            bind_port,
            &target_host,
            target_port,
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_port_forward(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    session_id: String,
    rule_id: String,
) -> Result<(), String> {
    sessions
        .remove_port_forward(&session_id, &rule_id)
        .await
        .map_err(|e| e.to_string())
}

/// Recent output of an open session, to redraw its terminal.
#[tauri::command]
fn get_scrollback(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<ScrollbackSnapshot, String> {
    sessions.scrollback(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_history(
    history: tauri::State<'_, Arc<History>>,
    profile_id: String,
) -> Result<Vec<HistoryEntry>, String> {
    let history = history.inner().clone();
    off_thread(move || history.list(&profile_id)).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn read_history(
    history: tauri::State<'_, Arc<History>>,
    profile_id: String,
    entry_id: String,
) -> Result<String, String> {
    let history = history.inner().clone();
    off_thread(move || history.read(&profile_id, &entry_id))
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Delete one saved session, or all of a profile's when `entry_id` is None.
#[tauri::command]
async fn delete_history(
    history: tauri::State<'_, Arc<History>>,
    profile_id: String,
    entry_id: Option<String>,
) -> Result<(), String> {
    let history = history.inner().clone();
    off_thread(move || match entry_id {
        Some(id) => history.delete(&profile_id, &id),
        None => history.delete_profile(&profile_id),
    })
    .await
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn history_enabled(history: tauri::State<'_, Arc<History>>) -> bool {
    history.enabled()
}

#[tauri::command]
fn set_history_enabled(
    history: tauri::State<'_, Arc<History>>,
    enabled: bool,
) -> Result<(), String> {
    history.set_enabled(enabled).map_err(|e| format!("{e:#}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let profile_store = Arc::new(
                ProfileStore::new(app.handle()).map_err(|e| e.to_string())?,
            );
            app.manage(profile_store);

            let data_dir = app.path().app_data_dir()?;
            let history = Arc::new(History::new(data_dir.join("history")));
            app.manage(history.clone());

            let session_manager = Arc::new(SessionManager::new(
                app.handle().clone(),
                data_dir.join("known_hosts"),
                history,
            ));
            app.manage(session_manager);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_profiles,
            save_profile,
            delete_profile,
            keychain_available,
            connect_ssh,
            trust_host_key,
            disconnect_ssh,
            write_terminal,
            resize_terminal,
            list_sftp_dir,
            download_sftp_file,
            upload_sftp_file,
            list_port_forwards,
            add_port_forward,
            remove_port_forward,
            get_scrollback,
            list_history,
            read_history,
            delete_history,
            history_enabled,
            set_history_enabled,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Sessions still open when the app closes go to history too.
            if let tauri::RunEvent::Exit = event {
                app.state::<Arc<SessionManager>>().save_all_history();
            }
        });
}
