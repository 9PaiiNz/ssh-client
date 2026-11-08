//! OS keychain access for saved passwords and key passphrases.
//!
//! Windows: Credential Manager. Linux: Secret Service (GNOME Keyring,
//! KWallet, KeePassXC, ...). If no keychain is reachable (e.g. a minimal
//! Linux session without a Secret Service daemon), `available()` is false
//! and callers fall back to the owner-only profiles file.
//!
//! All methods block (D-Bus round trips, possibly an unlock prompt), so call
//! them from a plain thread, never directly on the async runtime. The
//! Secret Service store also runs its own executor, which must not be nested
//! inside Tokio.

use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use keyring_core::{CredentialStore, Entry, Error as KeyringError};

const SERVICE: &str = "com.timle.ssh-client";

#[derive(Clone, Copy, Debug)]
pub enum SecretKind {
    Password,
    Passphrase,
}

impl SecretKind {
    fn suffix(self) -> &'static str {
        match self {
            SecretKind::Password => "password",
            SecretKind::Passphrase => "passphrase",
        }
    }
}

#[derive(Default)]
pub struct SecretStore {
    store: OnceLock<Option<Arc<CredentialStore>>>,
}

impl SecretStore {
    /// A store backed by a given credential store (`None` = no keychain).
    #[cfg(test)]
    pub fn with_store(store: Option<Arc<CredentialStore>>) -> Self {
        let s = Self::default();
        let _ = s.store.set(store);
        s
    }

    fn store(&self) -> Option<&Arc<CredentialStore>> {
        self.store.get_or_init(open_platform_store).as_ref()
    }

    /// True if the OS keychain can be used.
    pub fn available(&self) -> bool {
        self.store().is_some()
    }

    fn entry(&self, profile_id: &str, kind: SecretKind) -> Result<Entry> {
        let store = self.store().ok_or_else(|| anyhow!("no OS keychain available"))?;
        let user = format!("{profile_id}:{}", kind.suffix());
        Ok(store.build(SERVICE, &user, None)?)
    }

    pub fn get(&self, profile_id: &str, kind: SecretKind) -> Result<Option<String>> {
        match self.entry(profile_id, kind)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(e) => Err(anyhow!("keychain read failed: {e}")),
        }
    }

    pub fn set(&self, profile_id: &str, kind: SecretKind, secret: &str) -> Result<()> {
        self.entry(profile_id, kind)?
            .set_password(secret)
            .map_err(|e| anyhow!("keychain write failed: {e}"))
    }

    pub fn delete(&self, profile_id: &str, kind: SecretKind) -> Result<()> {
        match self.entry(profile_id, kind)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(e) => Err(anyhow!("keychain delete failed: {e}")),
        }
    }
}

fn open_platform_store() -> Option<Arc<CredentialStore>> {
    let store: Arc<CredentialStore> = match platform_store() {
        Ok(store) => store,
        Err(e) => {
            tracing::warn!("OS keychain unavailable, falling back to file storage: {e}");
            return None;
        }
    };
    // Creating the store can succeed even when the backing service is not
    // usable; probe with a lookup that needs no unlock prompt.
    let probe = store
        .build(SERVICE, "availability-probe", None)
        .and_then(|e| e.get_password());
    match probe {
        Ok(_) | Err(KeyringError::NoEntry) => Some(store),
        Err(e) => {
            tracing::warn!("OS keychain not usable, falling back to file storage: {e}");
            None
        }
    }
}

#[cfg(windows)]
fn platform_store() -> keyring_core::Result<Arc<CredentialStore>> {
    Ok(windows_native_keyring_store::Store::new()?)
}

#[cfg(target_os = "linux")]
fn platform_store() -> keyring_core::Result<Arc<CredentialStore>> {
    Ok(zbus_secret_service_keyring_store::Store::new()?)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn platform_store() -> keyring_core::Result<Arc<CredentialStore>> {
    Err(KeyringError::NotSupportedByStore(
        "no keychain backend for this platform".into(),
    ))
}

