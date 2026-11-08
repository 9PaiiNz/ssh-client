use std::sync::Arc;

use parking_lot::Mutex;
use russh::client;
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use russh::ChannelMsg;

use crate::models::HostKeyInfo;
use crate::ssh::known_hosts::{fingerprint, KnownHosts, Verdict};

/// Why the handshake rejected the server's host key, if it did.
#[derive(Debug, Clone)]
pub enum HostKeyRejection {
    Unknown(HostKeyInfo),
    Changed(HostKeyInfo),
}

pub struct SshClientHandler {
    host: String,
    port: u16,
    known_hosts: Arc<KnownHosts>,
    rejection: Arc<Mutex<Option<HostKeyRejection>>>,
}

impl SshClientHandler {
    pub fn new(
        host: String,
        port: u16,
        known_hosts: Arc<KnownHosts>,
        rejection: Arc<Mutex<Option<HostKeyRejection>>>,
    ) -> Self {
        Self {
            host,
            port,
            known_hosts,
            rejection,
        }
    }
}

impl client::Handler for SshClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // Host certificates are verified against the certified key itself;
        // CA-based trust is not supported.
        let key: PublicKey = match server_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(cert) => PublicKey::from(cert.public_key().clone()),
        };

        let verdict = self.known_hosts.verify(&self.host, self.port, &key);
        if matches!(verdict, Verdict::Trusted) {
            return Ok(true);
        }

        let info = HostKeyInfo {
            host: self.host.clone(),
            port: self.port,
            key_type: key.algorithm().to_string(),
            fingerprint: fingerprint(&key),
            key: key.to_openssh().unwrap_or_default(),
        };
        *self.rejection.lock() = Some(match verdict {
            Verdict::Changed => HostKeyRejection::Changed(info),
            _ => HostKeyRejection::Unknown(info),
        });
        Ok(false)
    }
}

pub fn decode_channel_data(msg: ChannelMsg) -> Option<Vec<u8>> {
    match msg {
        ChannelMsg::Data { data } => Some(data.to_vec()),
        ChannelMsg::ExtendedData { data, .. } => Some(data.to_vec()),
        _ => None,
    }
}

pub fn load_private_key(path: &str, passphrase: Option<&str>) -> anyhow::Result<PrivateKey> {
    let expanded = if path.starts_with("~/") || path.starts_with("~\\") {
        if let Some(home) = dirs::home_dir() {
            home.join(&path[2..]).to_string_lossy().to_string()
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };
    // Empty string must be treated as None — passing Some("") to an
    // unencrypted key makes ssh-key return "private key is already decrypted".
    let passphrase = passphrase.map(str::trim).filter(|p| !p.is_empty());
    Ok(russh::keys::load_secret_key(&expanded, passphrase)?)
}

pub async fn authenticate_with_key(
    handle: &mut client::Handle<SshClientHandler>,
    username: &str,
    key: PrivateKey,
) -> anyhow::Result<()> {
    // For RSA keys pick rsa-sha2-512/256 when the server supports it,
    // instead of legacy SHA-1 signatures.
    let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
    let result = handle
        .authenticate_publickey(username, PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg))
        .await?;
    if !result.success() {
        anyhow::bail!("key authentication rejected");
    }
    Ok(())
}

pub async fn authenticate_with_password(
    handle: &mut client::Handle<SshClientHandler>,
    username: &str,
    password: &str,
) -> anyhow::Result<()> {
    let result = handle.authenticate_password(username, password).await?;
    if !result.success() {
        anyhow::bail!("password authentication rejected");
    }
    Ok(())
}

