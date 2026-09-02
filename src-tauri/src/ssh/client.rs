use std::sync::Arc;

use async_trait::async_trait;
use russh::client;
use russh::ChannelMsg;
use russh_keys::key::PrivateKeyWithHashAlg;

pub struct SshClientHandler;

#[async_trait]
impl client::Handler for SshClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
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
    let expanded = if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            home.join(&path[2..]).to_string_lossy().to_string()
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };
    let key = russh_keys::load_secret_key(&expanded, passphrase)?;
    Ok(key)
}

pub async fn authenticate_with_key(
    handle: &mut client::Handle<SshClientHandler>,
    username: &str,
    key: russh_keys::key::KeyPair,
) -> anyhow::Result<()> {
    let auth_res = handle
        .authenticate_publickey(
            username,
            PrivateKeyWithHashAlg::new(
                Arc::new(key),
                handle.best_supported_rsa_hash().await?.flatten(),
            ),
        )
        .await?;

    if !auth_res.success() {
        anyhow::bail!("key authentication rejected");
    }
    Ok(())
}

pub async fn authenticate_with_password(
    handle: &mut client::Handle<SshClientHandler>,
    username: &str,
    password: &str,
) -> anyhow::Result<()> {
    let auth_res = handle
        .authenticate_password(username, password)
        .await?;

    if !auth_res.success() {
        anyhow::bail!("password authentication rejected");
    }
    Ok(())
}
