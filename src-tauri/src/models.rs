use serde::{Deserialize, Serialize};

/// A profile as exchanged with the frontend. Secrets are write-only: the
/// frontend may send a new password/passphrase when saving, but listing
/// never returns them, only `has_password` / `has_passphrase`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthConfig {
    // rename_all on the enum only renames variant tags ("key"), not fields —
    // each variant needs its own rename_all so keyPath / keyPassphrase round-trip.
    #[serde(rename_all = "camelCase")]
    Password {
        /// On save: `None` keeps the saved password, `Some("")` clears it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
        #[serde(default)]
        has_password: bool,
    },
    #[serde(rename_all = "camelCase")]
    Key {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key_path: Option<String>,
        /// On save: `None` keeps the saved passphrase, `Some("")` clears it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key_passphrase: Option<String>,
        #[serde(default)]
        has_passphrase: bool,
    },
}

/// A profile as written to profiles.json. Secrets live in the OS keychain
/// (`*_in_keychain`); the inline fields are only used when no keychain is
/// available, or hold plaintext from older versions awaiting migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredProfile {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: StoredAuth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StoredAuth {
    #[serde(rename_all = "camelCase")]
    Password {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        password_in_keychain: bool,
    },
    #[serde(rename_all = "camelCase")]
    Key {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key_path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key_passphrase: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        passphrase_in_keychain: bool,
    },
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Everything needed to open a connection, resolved by the backend from a
/// saved profile (the frontend only sends the profile id).
#[derive(Debug, Clone)]
pub struct ConnectParams {
    pub profile_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: ResolvedAuth,
}

#[derive(Debug, Clone)]
pub enum ResolvedAuth {
    Password(String),
    Key {
        path: String,
        passphrase: Option<String>,
    },
}

/// Host key details shown to the user when a key is unknown or changed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyInfo {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    /// OpenSSH-encoded public key, passed back to `trust_host_key`.
    pub key: String,
}

/// Error returned by `connect_ssh`, structured so the UI can offer to trust
/// an unknown host key or warn about a changed one.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConnectError {
    UnknownHostKey(HostKeyInfo),
    HostKeyChanged(HostKeyInfo),
    Other { message: String },
}

impl From<anyhow::Error> for ConnectError {
    fn from(e: anyhow::Error) -> Self {
        ConnectError::Other {
            message: format!("{e:#}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortForwardRule {
    pub id: String,
    pub session_id: String,
    pub forward_type: String,
    pub bind_host: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub transfer_id: String,
    pub session_id: String,
    pub file_name: String,
    pub direction: String,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusEvent {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
