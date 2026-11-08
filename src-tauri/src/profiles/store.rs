use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use tauri::{AppHandle, Manager};

use super::secrets::{SecretKind, SecretStore};
use crate::fsutil;
use crate::models::{
    AuthConfig, ConnectParams, ConnectionProfile, ResolvedAuth, StoredAuth, StoredProfile,
};

/// Saved connection profiles. Secrets go to the OS keychain when one is
/// available, otherwise into profiles.json, which is kept owner-only.
///
/// Methods may block on the keychain, so call them off the async runtime
/// (see `crate::off_thread`).
pub struct ProfileStore {
    path: PathBuf,
    secrets: SecretStore,
    /// Serializes read-modify-write cycles of profiles.json.
    lock: Mutex<()>,
}

impl ProfileStore {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let dir = app
            .path()
            .app_data_dir()
            .context("failed to resolve app data dir")?;
        fs::create_dir_all(&dir)?;
        Ok(Self::with_path(dir.join("profiles.json"), SecretStore::default()))
    }

    fn with_path(path: PathBuf, secrets: SecretStore) -> Self {
        // Older versions wrote this file world-readable.
        fsutil::restrict_file(&path);
        Self {
            path,
            secrets,
            lock: Mutex::new(()),
        }
    }

    pub fn keychain_available(&self) -> bool {
        self.secrets.available()
    }

    /// List profiles for the UI (without secrets). Also migrates plaintext
    /// secrets from older versions into the keychain.
    pub fn list(&self) -> Result<Vec<ConnectionProfile>> {
        let _guard = self.lock.lock().map_err(|_| anyhow!("profile store lock poisoned"))?;
        let mut profiles = self.read_all()?;
        if self.migrate_to_keychain(&mut profiles) {
            self.write_all(&profiles)?;
        }
        Ok(profiles.iter().map(to_view).collect())
    }

    pub fn save(&self, profile: ConnectionProfile) -> Result<()> {
        let _guard = self.lock.lock().map_err(|_| anyhow!("profile store lock poisoned"))?;
        let mut profiles = self.read_all()?;
        let old_auth = profiles
            .iter()
            .find(|p| p.id == profile.id)
            .map(|p| p.auth.clone());
        let id = profile.id.clone();

        let auth = match profile.auth {
            AuthConfig::Password { password, .. } => {
                // Switching away from key auth: forget the old passphrase.
                if let Some(StoredAuth::Key { passphrase_in_keychain: true, .. }) = &old_auth {
                    self.forget(&id, SecretKind::Passphrase);
                }
                let (password, password_in_keychain) = match password {
                    None => match old_auth {
                        Some(StoredAuth::Password { password, password_in_keychain }) => {
                            (password, password_in_keychain)
                        }
                        _ => (None, false),
                    },
                    Some(new) => self.replace_secret(&id, SecretKind::Password, new),
                };
                StoredAuth::Password { password, password_in_keychain }
            }
            AuthConfig::Key { key_path, key_passphrase, .. } => {
                if let Some(StoredAuth::Password { password_in_keychain: true, .. }) = &old_auth {
                    self.forget(&id, SecretKind::Password);
                }
                let (key_passphrase, passphrase_in_keychain) = match key_passphrase {
                    None => match old_auth {
                        Some(StoredAuth::Key { key_passphrase, passphrase_in_keychain, .. }) => {
                            (key_passphrase, passphrase_in_keychain)
                        }
                        _ => (None, false),
                    },
                    Some(new) => self.replace_secret(&id, SecretKind::Passphrase, new),
                };
                StoredAuth::Key {
                    key_path: key_path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()),
                    key_passphrase,
                    passphrase_in_keychain,
                }
            }
        };

        let stored = StoredProfile {
            id: profile.id,
            name: profile.name,
            group: profile.group.filter(|g| !g.trim().is_empty()),
            host: profile.host.trim().to_string(),
            port: profile.port,
            username: profile.username,
            auth,
        };
        match profiles.iter_mut().find(|p| p.id == stored.id) {
            Some(existing) => *existing = stored,
            None => profiles.push(stored),
        }
        self.write_all(&profiles)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let _guard = self.lock.lock().map_err(|_| anyhow!("profile store lock poisoned"))?;
        let mut profiles = self.read_all()?;
        if let Some(p) = profiles.iter().find(|p| p.id == id) {
            match p.auth {
                StoredAuth::Password { password_in_keychain: true, .. } => {
                    self.forget(id, SecretKind::Password)
                }
                StoredAuth::Key { passphrase_in_keychain: true, .. } => {
                    self.forget(id, SecretKind::Passphrase)
                }
                _ => {}
            }
        }
        profiles.retain(|p| p.id != id);
        self.write_all(&profiles)
    }

    /// Look up everything needed to connect, including secrets.
    pub fn resolve(&self, id: &str) -> Result<ConnectParams> {
        let profile = {
            let _guard = self.lock.lock().map_err(|_| anyhow!("profile store lock poisoned"))?;
            self.read_all()?
                .into_iter()
                .find(|p| p.id == id)
                .ok_or_else(|| anyhow!("profile not found"))?
        };
        let auth = match profile.auth {
            StoredAuth::Password { password, password_in_keychain } => {
                let password = match password {
                    Some(pw) => pw,
                    None if password_in_keychain => self
                        .secrets
                        .get(id, SecretKind::Password)?
                        .ok_or_else(|| anyhow!("saved password is missing from the keychain; edit the connection and enter it again"))?,
                    None => String::new(),
                };
                ResolvedAuth::Password(password)
            }
            StoredAuth::Key { key_path, key_passphrase, passphrase_in_keychain } => {
                let path = key_path.ok_or_else(|| anyhow!("key path required"))?;
                let passphrase = match key_passphrase {
                    Some(p) => Some(p),
                    None if passphrase_in_keychain => Some(
                        self.secrets
                            .get(id, SecretKind::Passphrase)?
                            .ok_or_else(|| anyhow!("saved key passphrase is missing from the keychain; edit the connection and enter it again"))?,
                    ),
                    None => None,
                };
                ResolvedAuth::Key { path, passphrase }
            }
        };
        Ok(ConnectParams {
            host: profile.host,
            port: profile.port,
            username: profile.username,
            auth,
        })
    }

    /// Store a new secret value; empty clears it. Returns the
    /// (inline value, in_keychain) pair to record in profiles.json.
    fn replace_secret(&self, id: &str, kind: SecretKind, value: String) -> (Option<String>, bool) {
        if value.is_empty() {
            self.forget(id, kind);
            return (None, false);
        }
        if self.secrets.available() {
            match self.secrets.set(id, kind, &value) {
                Ok(()) => return (None, true),
                Err(e) => tracing::warn!("{e:#}; saving to the profiles file instead"),
            }
        }
        (Some(value), false)
    }

    fn forget(&self, id: &str, kind: SecretKind) {
        if self.secrets.available() {
            if let Err(e) = self.secrets.delete(id, kind) {
                tracing::warn!("{e:#}");
            }
        }
    }

    /// Move plaintext secrets into the keychain. Returns true if anything changed.
    fn migrate_to_keychain(&self, profiles: &mut [StoredProfile]) -> bool {
        let has_plaintext = profiles.iter().any(|p| match &p.auth {
            StoredAuth::Password { password, .. } => password.is_some(),
            StoredAuth::Key { key_passphrase, .. } => key_passphrase.is_some(),
        });
        if !has_plaintext || !self.secrets.available() {
            return false;
        }
        let mut changed = false;
        for p in profiles.iter_mut() {
            let (inline, in_keychain, kind) = match &mut p.auth {
                StoredAuth::Password { password, password_in_keychain } => {
                    (password, password_in_keychain, SecretKind::Password)
                }
                StoredAuth::Key { key_passphrase, passphrase_in_keychain, .. } => {
                    (key_passphrase, passphrase_in_keychain, SecretKind::Passphrase)
                }
            };
            let Some(secret) = inline.as_deref() else { continue };
            if secret.is_empty() {
                *inline = None;
                changed = true;
                continue;
            }
            match self.secrets.set(&p.id, kind, secret) {
                Ok(()) => {
                    *inline = None;
                    *in_keychain = true;
                    changed = true;
                }
                Err(e) => tracing::warn!("migration of profile {} skipped: {e:#}", p.id),
            }
        }
        changed
    }

    fn read_all(&self) -> Result<Vec<StoredProfile>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let data = fs::read_to_string(&self.path)?;
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(&data)?)
    }

    fn write_all(&self, profiles: &[StoredProfile]) -> Result<()> {
        let data = serde_json::to_string_pretty(profiles)?;
        fsutil::write_private(&self.path, data.as_bytes())
    }
}

fn to_view(p: &StoredProfile) -> ConnectionProfile {
    let auth = match &p.auth {
        StoredAuth::Password { password, password_in_keychain } => AuthConfig::Password {
            password: None,
            has_password: *password_in_keychain || password.as_deref().is_some_and(|s| !s.is_empty()),
        },
        StoredAuth::Key { key_path, key_passphrase, passphrase_in_keychain } => AuthConfig::Key {
            key_path: key_path.clone(),
            key_passphrase: None,
            has_passphrase: *passphrase_in_keychain
                || key_passphrase.as_deref().is_some_and(|s| !s.is_empty()),
        },
    };
    ConnectionProfile {
        id: p.id.clone(),
        name: p.name.clone(),
        group: p.group.clone(),
        host: p.host.clone(),
        port: p.port,
        username: p.username.clone(),
        auth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn tmp_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("profiles-test-{}", uuid::Uuid::new_v4()))
            .join("profiles.json")
    }

    fn with_keychain(path: PathBuf) -> ProfileStore {
        let mock: Arc<keyring_core::CredentialStore> = keyring_core::mock::Store::new().unwrap();
        ProfileStore::with_path(path, SecretStore::with_store(Some(mock)))
    }

    fn without_keychain(path: PathBuf) -> ProfileStore {
        ProfileStore::with_path(path, SecretStore::with_store(None))
    }

    fn profile(auth: AuthConfig) -> ConnectionProfile {
        ConnectionProfile {
            id: "p1".into(),
            name: "test".into(),
            group: None,
            host: " example.com ".into(),
            port: 22,
            username: "me".into(),
            auth,
        }
    }

    fn pw(p: Option<&str>) -> AuthConfig {
        AuthConfig::Password { password: p.map(Into::into), has_password: false }
    }

    fn password_of(store: &ProfileStore) -> String {
        match store.resolve("p1").unwrap().auth {
            ResolvedAuth::Password(p) => p,
            other => panic!("unexpected auth {other:?}"),
        }
    }

    #[test]
    fn keychain_password_never_hits_disk_or_ui() {
        let path = tmp_path();
        let store = with_keychain(path.clone());
        store.save(profile(pw(Some("hunter2")))).unwrap();

        let file = fs::read_to_string(&path).unwrap();
        assert!(!file.contains("hunter2"), "password written to disk: {file}");
        assert!(file.contains("passwordInKeychain"));

        let listed = store.list().unwrap();
        let json = serde_json::to_string(&listed).unwrap();
        assert!(!json.contains("hunter2"), "password sent to UI: {json}");
        assert!(matches!(listed[0].auth, AuthConfig::Password { has_password: true, .. }));

        assert_eq!(password_of(&store), "hunter2");
        assert_eq!(store.resolve("p1").unwrap().host, "example.com");
    }

    #[test]
    fn none_keeps_empty_clears() {
        let store = with_keychain(tmp_path());
        store.save(profile(pw(Some("hunter2")))).unwrap();
        // Editing other fields without retyping the password keeps it.
        store.save(profile(pw(None))).unwrap();
        assert_eq!(password_of(&store), "hunter2");
        // Explicitly clearing removes it.
        store.save(profile(pw(Some("")))).unwrap();
        assert_eq!(password_of(&store), "");
        assert!(matches!(
            store.list().unwrap()[0].auth,
            AuthConfig::Password { has_password: false, .. }
        ));
    }

    #[test]
    fn switching_auth_type_forgets_old_secret() {
        let store = with_keychain(tmp_path());
        store.save(profile(pw(Some("hunter2")))).unwrap();
        store
            .save(profile(AuthConfig::Key {
                key_path: Some("~/.ssh/id_ed25519".into()),
                key_passphrase: Some("phrase".into()),
                has_passphrase: false,
            }))
            .unwrap();
        assert!(store.secrets.get("p1", SecretKind::Password).unwrap().is_none());
        match store.resolve("p1").unwrap().auth {
            ResolvedAuth::Key { path, passphrase } => {
                assert_eq!(path, "~/.ssh/id_ed25519");
                assert_eq!(passphrase.as_deref(), Some("phrase"));
            }
            other => panic!("unexpected auth {other:?}"),
        }
        store.delete("p1").unwrap();
        assert!(store.secrets.get("p1", SecretKind::Passphrase).unwrap().is_none());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn migrates_plaintext_from_old_versions() {
        let path = tmp_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // profiles.json as written by v0.1.x
        fs::write(
            &path,
            r#"[{"id":"p1","name":"old","host":"h","port":22,"username":"u","auth":{"type":"password","password":"hunter2"}}]"#,
        )
        .unwrap();
        let store = with_keychain(path.clone());
        store.list().unwrap();
        let file = fs::read_to_string(&path).unwrap();
        assert!(!file.contains("hunter2"), "plaintext left after migration: {file}");
        assert_eq!(password_of(&store), "hunter2");
    }

    #[test]
    fn falls_back_to_owner_only_file_without_keychain() {
        let path = tmp_path();
        let store = without_keychain(path.clone());
        store.save(profile(pw(Some("hunter2")))).unwrap();
        assert_eq!(password_of(&store), "hunter2");
        let json = serde_json::to_string(&store.list().unwrap()).unwrap();
        assert!(!json.contains("hunter2"), "password sent to UI: {json}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
            let dir_mode = fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700);
        }
    }
}
