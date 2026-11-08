//! Server host key verification (trust on first use).
//!
//! The app keeps its own `known_hosts` file (OpenSSH format, plain host
//! names) in the app data dir. The user's `~/.ssh/known_hosts` is consulted
//! read-only, so hosts already trusted with OpenSSH don't prompt again.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use russh::keys::known_hosts::known_host_keys_path;
use russh::keys::{HashAlg, PublicKey};

#[derive(Debug, Clone)]
pub enum Verdict {
    /// The key matches a recorded key for this host.
    Trusted,
    /// No key is recorded for this host yet.
    Unknown,
    /// Keys are recorded for this host, but none match. Possible MITM.
    Changed,
}

/// `host` for port 22, `[host]:port` otherwise, as OpenSSH writes it.
fn host_pattern(host: &str, port: u16) -> String {
    let host = host.trim().to_ascii_lowercase();
    if port == 22 {
        host
    } else {
        format!("[{host}]:{port}")
    }
}

pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

pub struct KnownHosts {
    path: PathBuf,
}

impl KnownHosts {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn verify(&self, host: &str, port: u16, key: &PublicKey) -> Verdict {
        let ours = self.recorded_keys(host, port);
        if !ours.is_empty() {
            return judge(&ours, key);
        }
        // Fall back to the user's OpenSSH file (read-only). Parse errors there
        // (e.g. unsupported key types) just mean "no information".
        if let Some(home) = dirs::home_dir() {
            let openssh = home.join(".ssh").join("known_hosts");
            let theirs: Vec<PublicKey> = known_host_keys_path(host.trim(), port, &openssh)
                .map(|v| v.into_iter().map(|(_, k)| k).collect())
                .unwrap_or_default();
            if !theirs.is_empty() {
                return judge(&theirs, key);
            }
        }
        Verdict::Unknown
    }

    /// Record `key` for the host. With `replace`, previously recorded keys
    /// for that host are removed first (used after a confirmed key change).
    pub fn trust(&self, host: &str, port: u16, key: &PublicKey, replace: bool) -> Result<()> {
        let pattern = host_pattern(host, port);
        let existing = fs::read_to_string(&self.path).unwrap_or_default();
        let mut out = String::new();
        for line in existing.lines() {
            if replace && line_host_matches(line, &pattern) {
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        let key_line = key
            .to_openssh()
            .context("failed to encode host key")?;
        // Drop the comment field; known_hosts lines are "host algo base64".
        let mut parts = key_line.split_whitespace();
        let (algo, data) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
        out.push_str(&format!("{pattern} {algo} {data}\n"));
        write_atomic(&self.path, out.as_bytes())
    }

    fn recorded_keys(&self, host: &str, port: u16) -> Vec<PublicKey> {
        let pattern = host_pattern(host, port);
        let Ok(content) = fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        content
            .lines()
            .filter(|l| line_host_matches(l, &pattern))
            .filter_map(|l| {
                let mut parts = l.split_whitespace().skip(1);
                let algo = parts.next()?;
                let data = parts.next()?;
                PublicKey::from_openssh(&format!("{algo} {data}")).ok()
            })
            .collect()
    }
}

fn judge(recorded: &[PublicKey], key: &PublicKey) -> Verdict {
    // Compare key material only; comments don't matter.
    if recorded.iter().any(|k| k.key_data() == key.key_data()) {
        Verdict::Trusted
    } else {
        // Unlike russh's own check, a key of a *different* algorithm also
        // counts as changed, so an attacker can't dodge the warning by
        // offering another key type.
        Verdict::Changed
    }
}

fn line_host_matches(line: &str, pattern: &str) -> bool {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return false;
    }
    line.split_whitespace()
        .next()
        .map(|hosts| hosts.split(',').any(|h| h.eq_ignore_ascii_case(pattern)))
        .unwrap_or(false)
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    crate::fsutil::write_private(path, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_A: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
    const KEY_B: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBt4rq3ftqvsOqgUSAm5a7VCxw6cGFkABIJzGa0lmJjP";

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kh-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("known_hosts")
    }

    #[test]
    fn tofu_flow() {
        let kh = KnownHosts::new(tmp());
        let a = PublicKey::from_openssh(KEY_A).unwrap();
        let b = PublicKey::from_openssh(KEY_B).unwrap();

        // Use a host that won't be in the developer's ~/.ssh/known_hosts.
        let host = "kh-test.invalid";
        assert!(matches!(kh.verify(host, 2222, &a), Verdict::Unknown));
        kh.trust(host, 2222, &a, false).unwrap();
        assert!(matches!(kh.verify(host, 2222, &a), Verdict::Trusted));
        assert!(matches!(kh.verify("KH-TEST.invalid", 2222, &a), Verdict::Trusted));
        assert!(matches!(kh.verify(host, 2222, &b), Verdict::Changed));
        // Different port is a different host.
        assert!(matches!(kh.verify(host, 22, &a), Verdict::Unknown));

        kh.trust(host, 2222, &b, true).unwrap();
        assert!(matches!(kh.verify(host, 2222, &b), Verdict::Trusted));
        assert!(matches!(kh.verify(host, 2222, &a), Verdict::Changed));
    }
}
