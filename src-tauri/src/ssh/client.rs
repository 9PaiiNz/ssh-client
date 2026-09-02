use std::sync::Arc;

use async_trait::async_trait;
use russh::client;
use russh::ChannelMsg;

pub struct SshClientHandler;

#[async_trait]
impl client::Handler for SshClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // Accept all host keys in v1; TOFU UI can be added later.
        Ok(true)
    }
}

pub fn decode_channel_data(msg: ChannelMsg) -> Option<Vec<u8>> {
    match msg {
        ChannelMsg::Data { data } => Some(data.to_vec()),
        ChannelMsg::ExtendedData { data, .. } => Some(data.to_vec()),
        _ => None,
    }
}

pub fn load_private_key(
    path: &str,
    passphrase: Option<&str>,
) -> anyhow::Result<russh_keys::key::KeyPair> {
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
    Ok(russh_keys::load_secret_key(&expanded, passphrase)?)
}

pub async fn authenticate_with_key(
    handle: &mut client::Handle<SshClientHandler>,
    username: &str,
    key: russh_keys::key::KeyPair,
) -> anyhow::Result<()> {
    let ok = handle
        .authenticate_publickey(username, Arc::new(key))
        .await?;
    if !ok {
        anyhow::bail!("key authentication rejected");
    }
    Ok(())
}

pub async fn authenticate_with_password(
    handle: &mut client::Handle<SshClientHandler>,
    username: &str,
    password: &str,
) -> anyhow::Result<()> {
    let ok = handle.authenticate_password(username, password).await?;
    if !ok {
        anyhow::bail!("password authentication rejected");
    }
    Ok(())
}
