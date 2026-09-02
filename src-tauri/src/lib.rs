mod models;
mod profiles;
mod ssh;

use std::sync::Arc;

use models::{
    ConnectParams, ConnectionProfile, PortForwardRule, SftpEntry, TransferProgress,
};
use profiles::ProfileStore;
use ssh::SessionManager;
use tauri::Manager;

#[tauri::command]
fn list_profiles(state: tauri::State<'_, Arc<ProfileStore>>) -> Result<Vec<ConnectionProfile>, String> {
    state.list().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_profile(
    state: tauri::State<'_, Arc<ProfileStore>>,
    profile: ConnectionProfile,
) -> Result<(), String> {
    state.save(&profile).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_profile(state: tauri::State<'_, Arc<ProfileStore>>, id: String) -> Result<(), String> {
    state.delete(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn connect_ssh(
    sessions: tauri::State<'_, Arc<SessionManager>>,
    params: ConnectParams,
) -> Result<String, String> {
    sessions.connect(params).await.map_err(|e| e.to_string())
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let profile_store = Arc::new(
                ProfileStore::new(app.handle()).map_err(|e| e.to_string())?,
            );
            app.manage(profile_store);

            let session_manager = Arc::new(SessionManager::new(app.handle().clone()));
            app.manage(session_manager);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_profiles,
            save_profile,
            delete_profile,
            connect_ssh,
            disconnect_ssh,
            write_terminal,
            resize_terminal,
            list_sftp_dir,
            download_sftp_file,
            upload_sftp_file,
            list_port_forwards,
            add_port_forward,
            remove_port_forward,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
