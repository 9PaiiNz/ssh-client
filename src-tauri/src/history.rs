//! Terminal scrollback for live sessions and saved output of ended sessions.
//!
//! Every session keeps its most recent output in memory ([`Scrollback`]).
//! When the session ends (tab closed, server disconnect or app exit) that
//! output is written to `<app data>/history/<profile id>/<entry id>.log`,
//! with metadata next to it in `<entry id>.json`. Only the newest
//! [`KEEP_PER_PROFILE`] entries per profile are kept.
//!
//! Saved output can contain sensitive data (anything printed in the shell),
//! so files are owner-only and history can be turned off.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::fsutil::write_private;

/// Output kept per session (bytes of UTF-8 text).
pub const SCROLLBACK_BYTES: usize = 1024 * 1024;
/// Saved sessions kept per profile.
pub const KEEP_PER_PROFILE: usize = 10;

/// The most recent output of a session, as text.
#[derive(Default)]
pub struct Scrollback {
    state: Mutex<ScrollState>,
}

#[derive(Default)]
struct ScrollState {
    text: String,
    /// Total bytes ever appended; identifies a position in the output stream.
    end: u64,
}

impl Scrollback {
    /// Append output and return the stream position after it.
    pub fn push(&self, chunk: &str) -> u64 {
        let mut s = self.state.lock();
        s.text.push_str(chunk);
        s.end += chunk.len() as u64;
        // Trim in bulk so a busy session doesn't memmove the buffer per chunk.
        if s.text.len() > 2 * SCROLLBACK_BYTES {
            let cut = ceil_char_boundary(&s.text, s.text.len() - SCROLLBACK_BYTES);
            s.text.drain(..cut);
        }
        s.end
    }

    /// The kept output and the stream position it ends at.
    pub fn snapshot(&self) -> (String, u64) {
        let s = self.state.lock();
        let start = ceil_char_boundary(&s.text, s.text.len().saturating_sub(SCROLLBACK_BYTES));
        (s.text[start..].to_string(), s.end)
    }
}

fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Decodes a byte stream as UTF-8, holding back a multi-byte character that
/// is split across two packets instead of turning it into garbage.
#[derive(Default)]
pub struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    pub fn decode(&mut self, data: &[u8]) -> String {
        self.pending.extend_from_slice(data);
        let mut out = String::new();
        let consumed = {
            let mut rest: &[u8] = &self.pending;
            loop {
                match std::str::from_utf8(rest) {
                    Ok(s) => {
                        out.push_str(s);
                        rest = &[];
                        break;
                    }
                    Err(e) => {
                        let (valid, after) = rest.split_at(e.valid_up_to());
                        // `valid` is valid UTF-8 by construction.
                        out.push_str(std::str::from_utf8(valid).unwrap_or_default());
                        match e.error_len() {
                            Some(n) => {
                                out.push('\u{FFFD}');
                                rest = &after[n..];
                            }
                            // Incomplete character at the end: wait for more.
                            None => {
                                rest = after;
                                break;
                            }
                        }
                    }
                }
            }
            self.pending.len() - rest.len()
        };
        self.pending.drain(..consumed);
        out
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EndReason {
    /// The user closed the tab.
    Closed,
    /// The server ended the session or the connection dropped.
    Disconnected,
    /// The app was closed while the session was open.
    AppExit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub profile_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Unix time in milliseconds.
    pub started_at: u64,
    pub ended_at: u64,
    pub end_reason: EndReason,
    /// Size of the saved output in bytes.
    pub size: u64,
}

#[derive(Serialize, Deserialize)]
struct Settings {
    enabled: bool,
}

/// Saved session output on disk.
pub struct History {
    dir: PathBuf,
    enabled: AtomicBool,
}

impl History {
    pub fn new(dir: PathBuf) -> Self {
        let enabled = fs::read(dir.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .is_none_or(|s| s.enabled);
        Self {
            dir,
            enabled: AtomicBool::new(enabled),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        let data = serde_json::to_vec(&Settings { enabled })?;
        write_private(&self.dir.join("settings.json"), &data)?;
        self.enabled.store(enabled, Ordering::Relaxed);
        Ok(())
    }

    fn profile_dir(&self, profile_id: &str) -> Result<PathBuf> {
        Ok(self.dir.join(safe_name(profile_id)?))
    }

    /// Saved sessions of a profile, newest first.
    pub fn list(&self, profile_id: &str) -> Result<Vec<HistoryEntry>> {
        let dir = self.profile_dir(profile_id)?;
        let mut entries = read_entries(&dir);
        entries.sort_by_key(|e| std::cmp::Reverse(e.ended_at));
        Ok(entries)
    }

    pub fn read(&self, profile_id: &str, entry_id: &str) -> Result<String> {
        let path = self.profile_dir(profile_id)?.join(format!("{}.log", safe_name(entry_id)?));
        let bytes = fs::read(&path).context("saved session not found")?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    pub fn delete(&self, profile_id: &str, entry_id: &str) -> Result<()> {
        let dir = self.profile_dir(profile_id)?;
        let id = safe_name(entry_id)?;
        remove_entry(&dir, id);
        Ok(())
    }

    /// Delete all saved sessions of a profile.
    pub fn delete_profile(&self, profile_id: &str) -> Result<()> {
        let dir = self.profile_dir(profile_id)?;
        match fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(e).with_context(|| format!("failed to delete {}", dir.display()))
            }
            _ => Ok(()),
        }
    }

    fn save(&self, mut entry: HistoryEntry, text: &str) -> Result<()> {
        let dir = self.profile_dir(&entry.profile_id)?;
        entry.size = text.len() as u64;
        write_private(&dir.join(format!("{}.log", entry.id)), text.as_bytes())?;
        write_private(
            &dir.join(format!("{}.json", entry.id)),
            &serde_json::to_vec_pretty(&entry)?,
        )?;

        let mut entries = read_entries(&dir);
        entries.sort_by_key(|e| std::cmp::Reverse(e.ended_at));
        for old in entries.iter().skip(KEEP_PER_PROFILE) {
            remove_entry(&dir, &old.id);
        }
        Ok(())
    }
}

fn read_entries(dir: &Path) -> Vec<HistoryEntry> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    read_dir
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<HistoryEntry>(&b).ok())
        .filter(|e| safe_name(&e.id).is_ok())
        .collect()
}

fn remove_entry(dir: &Path, id: &str) {
    let _ = fs::remove_file(dir.join(format!("{id}.log")));
    let _ = fs::remove_file(dir.join(format!("{id}.json")));
}

/// Only allow ids that are safe as a single path component.
fn safe_name(id: &str) -> Result<&str> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        bail!("invalid id");
    }
    Ok(id)
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Output and details of one live session, saved once when it ends.
pub struct SessionLog {
    pub scrollback: Scrollback,
    profile_id: String,
    host: String,
    port: u16,
    username: String,
    started_at: u64,
    history: Arc<History>,
    /// Held while saving, so an app exit waits for a save in progress.
    saved: Mutex<bool>,
}

impl SessionLog {
    pub fn new(
        history: Arc<History>,
        profile_id: String,
        host: String,
        port: u16,
        username: String,
    ) -> Self {
        Self {
            scrollback: Scrollback::default(),
            profile_id,
            host,
            port,
            username,
            started_at: now_ms(),
            history,
            saved: Mutex::new(false),
        }
    }

    /// Save the output to history (first call only). Blocks on file I/O.
    pub fn save(&self, reason: EndReason) {
        let mut saved = self.saved.lock();
        if *saved {
            return;
        }
        *saved = true;
        if !self.history.enabled() {
            return;
        }
        let (text, _) = self.scrollback.snapshot();
        if text.trim().is_empty() {
            return;
        }
        let ended_at = now_ms();
        let entry = HistoryEntry {
            id: format!("{ended_at}-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            profile_id: self.profile_id.clone(),
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            started_at: self.started_at,
            ended_at,
            end_reason: reason,
            size: 0,
        };
        if let Err(e) = self.history.save(entry, &text) {
            tracing::warn!("failed to save session history: {e:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_split_across_packets() {
        let mut d = Utf8Stream::default();
        let bytes = "aä€😀b".as_bytes();
        let mut out = String::new();
        for b in bytes {
            out += &d.decode(&[*b]);
        }
        assert_eq!(out, "aä€😀b");
        assert_eq!(d.decode(&[0xff, b'x']), "\u{FFFD}x");
    }

    #[test]
    fn scrollback_trims_on_char_boundary() {
        let sb = Scrollback::default();
        let chunk = "ä".repeat(1000);
        let mut end = 0;
        for _ in 0..3000 {
            end = sb.push(&chunk);
        }
        let (text, snap_end) = sb.snapshot();
        assert_eq!(end, snap_end);
        assert!(text.len() <= SCROLLBACK_BYTES);
        assert!(text.chars().all(|c| c == 'ä'));
    }

    #[test]
    fn save_list_read_prune() {
        let dir = std::env::temp_dir().join(format!("ssh-history-test-{}", uuid::Uuid::new_v4()));
        let history = Arc::new(History::new(dir.clone()));

        for i in 0..(KEEP_PER_PROFILE + 3) {
            let log = SessionLog::new(history.clone(), "p1".into(), "h".into(), 22, "u".into());
            log.scrollback.push(&format!("session {i}\r\n"));
            log.save(EndReason::Closed);
            log.save(EndReason::AppExit); // second save is ignored
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let entries = history.list("p1").unwrap();
        assert_eq!(entries.len(), KEEP_PER_PROFILE);
        let newest = history.read("p1", &entries[0].id).unwrap();
        assert_eq!(newest, format!("session {}\r\n", KEEP_PER_PROFILE + 2));
        assert_eq!(entries[0].end_reason, EndReason::Closed);

        assert!(history.read("../p1", &entries[0].id).is_err());
        assert!(history.read("p1", "../../etc/passwd").is_err());

        history.set_enabled(false).unwrap();
        assert!(!History::new(dir.clone()).enabled());
        let log = SessionLog::new(history.clone(), "p2".into(), "h".into(), 22, "u".into());
        log.scrollback.push("secret\r\n");
        log.save(EndReason::Closed);
        assert!(history.list("p2").unwrap().is_empty());

        history.delete_profile("p1").unwrap();
        assert!(history.list("p1").unwrap().is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}
