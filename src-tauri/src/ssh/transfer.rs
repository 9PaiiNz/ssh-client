//! SFTP file and folder transfers with progress events.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::models::TransferProgress;

const CHUNK: usize = 64 * 1024;
const EMIT_EVERY: Duration = Duration::from_millis(150);

/// Progress of one transfer (a file or a whole folder), shown in the UI.
pub struct Transfer {
    emit: Box<dyn Fn(&TransferProgress) + Send + Sync>,
    progress: TransferProgress,
    last_emit: Instant,
}

impl Transfer {
    pub fn start(
        emit: impl Fn(&TransferProgress) + Send + Sync + 'static,
        session_id: &str,
        name: &str,
        direction: &str,
    ) -> Self {
        let t = Self {
            emit: Box::new(emit),
            progress: TransferProgress {
                transfer_id: Uuid::new_v4().to_string(),
                session_id: session_id.to_string(),
                file_name: name.to_string(),
                direction: direction.to_string(),
                bytes_transferred: 0,
                total_bytes: 0,
                status: "in_progress".to_string(),
                error: None,
            },
            last_emit: Instant::now(),
        };
        t.emit();
        t
    }

    fn emit(&self) {
        (self.emit)(&self.progress);
    }

    fn set_total(&mut self, total: u64) {
        self.progress.total_bytes = total;
        self.emit();
    }

    fn add(&mut self, n: usize) {
        self.progress.bytes_transferred += n as u64;
        if self.last_emit.elapsed() >= EMIT_EVERY {
            self.last_emit = Instant::now();
            self.emit();
        }
    }

    /// Report the final state and pass the result through.
    pub fn finish<T>(mut self, result: Result<T>) -> Result<T> {
        match &result {
            Ok(_) => self.progress.status = "completed".to_string(),
            Err(e) => {
                self.progress.status = "failed".to_string();
                self.progress.error = Some(format!("{e:#}"));
            }
        }
        self.emit();
        result
    }
}

/// Join a remote directory and a name with `/`.
pub fn remote_join(dir: &str, name: &str) -> String {
    format!("{}/{}", dir.trim_end_matches('/'), name)
}

/// Last component of a remote or local path.
pub fn base_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or("file")
}

/// A name from the server that is safe to use as one local path component.
fn safe_local_name(name: &str) -> Result<&str> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        bail!("refusing unsafe file name from server: {name:?}");
    }
    Ok(name)
}

/// `dir/name`, or `dir/name (2)` etc. if that already exists.
fn unique_local_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|i| dir.join(format!("{name} ({i})")))
        .find(|p| !p.exists())
        .expect("unbounded range")
}

async fn copy_down(sftp: &SftpSession, remote: &str, local: &Path, t: &mut Transfer) -> Result<()> {
    let mut src = sftp
        .open(remote)
        .await
        .with_context(|| format!("failed to open {remote}"))?;
    let mut dst = tokio::fs::File::create(local)
        .await
        .with_context(|| format!("failed to create {}", local.display()))?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = src.read(&mut buf).await.with_context(|| format!("failed to read {remote}"))?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n]).await.context("failed to write local file")?;
        t.add(n);
    }
    dst.flush().await?;
    Ok(())
}

async fn copy_up(sftp: &SftpSession, local: &Path, remote: &str, t: &mut Transfer) -> Result<()> {
    let mut src = tokio::fs::File::open(local)
        .await
        .with_context(|| format!("failed to open {}", local.display()))?;
    let mut dst = sftp
        .create(remote)
        .await
        .with_context(|| format!("failed to create {remote}"))?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = src.read(&mut buf).await.context("failed to read local file")?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n]).await.with_context(|| format!("failed to write {remote}"))?;
        t.add(n);
    }
    dst.shutdown().await.with_context(|| format!("failed to close {remote}"))?;
    Ok(())
}

/// Download one remote file to `local_path`.
pub async fn download_file(
    sftp: &SftpSession,
    remote_path: &str,
    local_path: &Path,
    t: &mut Transfer,
) -> Result<()> {
    let size = sftp
        .metadata(remote_path)
        .await
        .context("failed to stat remote file")?
        .size
        .unwrap_or(0);
    t.set_total(size);
    copy_down(sftp, remote_path, local_path, t).await
}

/// Download a remote folder with everything in it into `local_parent`.
/// Returns the created local folder (renamed to `name (2)` if it existed).
pub async fn download_dir(
    sftp: &SftpSession,
    remote_dir: &str,
    local_parent: &Path,
    t: &mut Transfer,
) -> Result<PathBuf> {
    let root = unique_local_path(local_parent, safe_local_name(base_name(remote_dir))?);

    // Walk the tree first so the progress bar knows the total size.
    // Symlinked folders are skipped to avoid loops; symlinked files are copied.
    let mut dirs = vec![(remote_dir.to_string(), root.clone())];
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let mut total = 0u64;
    let mut i = 0;
    while i < dirs.len() {
        let (remote, local) = dirs[i].clone();
        i += 1;
        let entries = sftp
            .read_dir(remote.as_str())
            .await
            .with_context(|| format!("failed to list {remote}"))?;
        for entry in entries {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let name = safe_local_name(&name)?.to_string();
            let path = remote_join(&remote, &name);
            let mut meta = entry.metadata();
            if meta.file_type().is_symlink() {
                match sftp.metadata(path.as_str()).await {
                    Ok(target) if !target.file_type().is_dir() => meta = target,
                    _ => continue,
                }
            }
            if meta.file_type().is_dir() {
                dirs.push((path, local.join(&name)));
            } else if meta.file_type().is_file() {
                total += meta.size.unwrap_or(0);
                files.push((path, local.join(&name)));
            }
            // Sockets, devices, pipes: nothing to download.
        }
    }
    t.set_total(total);

    for (_, local) in &dirs {
        tokio::fs::create_dir_all(local)
            .await
            .with_context(|| format!("failed to create {}", local.display()))?;
    }
    for (remote, local) in &files {
        copy_down(sftp, remote, local, t).await?;
    }
    Ok(root)
}

/// Upload local files and folders (recursively) into `remote_dir`.
/// Existing remote files are overwritten; existing folders are merged.
pub async fn upload(
    sftp: &SftpSession,
    local_path: &Path,
    remote_dir: &str,
    t: &mut Transfer,
) -> Result<()> {
    let name = local_path
        .file_name()
        .and_then(|n| n.to_str())
        .context("invalid local file name")?;
    let target = remote_join(remote_dir, name);

    let meta = tokio::fs::metadata(local_path)
        .await
        .with_context(|| format!("failed to read {}", local_path.display()))?;
    if !meta.is_dir() {
        t.set_total(meta.len());
        return copy_up(sftp, local_path, &target, t).await;
    }

    // Walk the local tree; symlinked folders are skipped to avoid loops.
    let mut dirs = vec![(local_path.to_path_buf(), target)];
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    let mut total = 0u64;
    let mut i = 0;
    while i < dirs.len() {
        let (local, remote) = dirs[i].clone();
        i += 1;
        let mut rd = tokio::fs::read_dir(&local)
            .await
            .with_context(|| format!("failed to list {}", local.display()))?;
        while let Some(entry) = rd.next_entry().await? {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                tracing::warn!("skipping non-UTF-8 file name in {}", local.display());
                continue;
            };
            let path = entry.path();
            let is_link = entry.file_type().await?.is_symlink();
            let Ok(meta) = tokio::fs::metadata(&path).await else {
                continue; // broken symlink
            };
            if meta.is_dir() {
                if !is_link {
                    dirs.push((path, remote_join(&remote, &name)));
                }
            } else if meta.is_file() {
                total += meta.len();
                files.push((path, remote_join(&remote, &name)));
            }
        }
    }
    t.set_total(total);

    for (_, remote) in &dirs {
        let exists = sftp.try_exists(remote.as_str()).await.unwrap_or(false);
        if !exists {
            sftp.create_dir(remote.as_str())
                .await
                .with_context(|| format!("failed to create {remote}"))?;
        }
    }
    for (local, remote) in &files {
        copy_up(sftp, local, remote, t).await?;
    }
    Ok(())
}

/// Delete a remote file, symlink or folder (with everything in it).
/// Symlinks are removed themselves, never followed. Returns the number of
/// removed entries.
pub async fn delete(sftp: &SftpSession, path: &str) -> Result<usize> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || base_name(trimmed) == "." || base_name(trimmed) == ".." {
        bail!("refusing to delete {path:?}");
    }
    let meta = sftp
        .symlink_metadata(trimmed)
        .await
        .with_context(|| format!("failed to read {trimmed}"))?;
    if !meta.file_type().is_dir() {
        sftp.remove_file(trimmed)
            .await
            .with_context(|| format!("failed to delete {trimmed}"))?;
        return Ok(1);
    }

    // Collect the tree, then delete files first and folders deepest first.
    let mut dirs = vec![trimmed.to_string()];
    let mut files = Vec::new();
    let mut i = 0;
    while i < dirs.len() {
        let dir = dirs[i].clone();
        i += 1;
        let entries = sftp
            .read_dir(dir.as_str())
            .await
            .with_context(|| format!("failed to list {dir}"))?;
        for entry in entries {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child = remote_join(&dir, &name);
            if entry.metadata().file_type().is_dir() {
                dirs.push(child);
            } else {
                files.push(child);
            }
        }
    }
    for file in &files {
        sftp.remove_file(file.as_str())
            .await
            .with_context(|| format!("failed to delete {file}"))?;
    }
    for dir in dirs.iter().rev() {
        sftp.remove_dir(dir.as_str())
            .await
            .with_context(|| format!("failed to delete folder {dir}"))?;
    }
    Ok(files.len() + dirs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(base_name("/var/log/"), "log");
        assert_eq!(base_name("/var/log"), "log");
        assert_eq!(base_name("C:\\Users\\me\\dir"), "dir");
        assert_eq!(base_name("/"), "file");
        assert_eq!(remote_join("/", "a"), "/a");
        assert_eq!(remote_join("/home/u/", "a"), "/home/u/a");
        assert!(safe_local_name("..").is_err());
        assert!(safe_local_name("a/b").is_err());
        assert!(safe_local_name("a\\b").is_err());
        assert!(safe_local_name("ok.txt").is_ok());
    }

    #[test]
    fn unique_names() {
        let dir = std::env::temp_dir().join(format!("ssh-unique-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        std::fs::create_dir_all(dir.join("logs (2)")).unwrap();
        assert_eq!(unique_local_path(&dir, "logs"), dir.join("logs (3)"));
        assert_eq!(unique_local_path(&dir, "new"), dir.join("new"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Round trip against a real SFTP server. Run with:
    /// `SFTP_TEST_ADDR=127.0.0.1:2222 SFTP_TEST_USER=me SFTP_TEST_KEY=/path/key \
    ///  cargo test -- --ignored sftp_round_trip`
    #[tokio::test]
    #[ignore]
    async fn sftp_round_trip() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;

        struct AcceptAll;
        impl russh::client::Handler for AcceptAll {
            type Error = anyhow::Error;
            async fn check_server_key(
                &mut self,
                _: &russh::keys::PublicKeyOrCertificate,
            ) -> Result<bool, Self::Error> {
                Ok(true)
            }
        }

        let addr = std::env::var("SFTP_TEST_ADDR").unwrap();
        let user = std::env::var("SFTP_TEST_USER").unwrap();
        let key = russh::keys::load_secret_key(std::env::var("SFTP_TEST_KEY").unwrap(), None).unwrap();
        let mut handle = russh::client::connect(Arc::new(Default::default()), addr, AcceptAll)
            .await
            .unwrap();
        let auth = handle
            .authenticate_publickey(
                user,
                russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None),
            )
            .await
            .unwrap();
        assert!(auth.success());
        let channel = handle.channel_open_session().await.unwrap();
        channel.request_subsystem(true, "sftp").await.unwrap();
        let sftp = SftpSession::new(channel.into_stream()).await.unwrap();

        // Local tree: files, nested and empty folders, a symlinked folder.
        let base = std::env::temp_dir().join(format!("sftp-rt-{}", Uuid::new_v4()));
        let src = base.join("src");
        std::fs::create_dir_all(src.join("sub/empty")).unwrap();
        std::fs::write(src.join("a.txt"), "hello").unwrap();
        let big: Vec<u8> = (0..1_500_000u32).map(|i| (i * 7 % 251) as u8).collect();
        std::fs::write(src.join("sub/b.bin"), &big).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(src.join("sub"), src.join("loop")).unwrap();
        let remote = base.join("remote");
        std::fs::create_dir_all(&remote).unwrap();
        let remote = remote.to_str().unwrap().to_string();

        let last = Arc::new(AtomicU64::new(0));
        let l = last.clone();
        let mut t = Transfer::start(
            move |p| {
                if p.status == "completed" {
                    assert_eq!(p.bytes_transferred, p.total_bytes);
                    l.store(p.total_bytes, Ordering::SeqCst);
                }
            },
            "s",
            "src",
            "upload",
        );
        let r = upload(&sftp, &src, &remote, &mut t).await;
        t.finish(r).unwrap();
        assert_eq!(last.load(Ordering::SeqCst), 5 + 1_500_000);
        let up = Path::new(&remote).join("src");
        assert_eq!(std::fs::read(up.join("sub/b.bin")).unwrap(), big);
        assert!(up.join("sub/empty").is_dir());
        assert!(!up.join("loop").exists(), "symlinked folder must be skipped");

        // Download it back, twice: the second lands in "src (2)".
        let dl = base.join("dl");
        std::fs::create_dir_all(&dl).unwrap();
        for expected in ["src", "src (2)"] {
            let mut t = Transfer::start(|_| {}, "s", "src", "download");
            let r = download_dir(&sftp, &format!("{remote}/src"), &dl, &mut t).await;
            let root = t.finish(r).unwrap();
            assert_eq!(root, dl.join(expected));
            assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "hello");
            assert_eq!(std::fs::read(root.join("sub/b.bin")).unwrap(), big);
            assert!(root.join("sub/empty").is_dir());
        }

        // Single file download, and a failing one reports an error.
        let mut t = Transfer::start(|_| {}, "s", "a.txt", "download");
        let r = download_file(&sftp, &format!("{remote}/src/a.txt"), &dl.join("one.txt"), &mut t).await;
        t.finish(r).unwrap();
        assert_eq!(std::fs::read_to_string(dl.join("one.txt")).unwrap(), "hello");
        let failed = Arc::new(AtomicU64::new(0));
        let f = failed.clone();
        let mut t = Transfer::start(
            move |p| {
                if p.status == "failed" {
                    f.store(1, Ordering::SeqCst);
                }
            },
            "s",
            "missing",
            "download",
        );
        let r = download_file(&sftp, "/does/not/exist", &dl.join("x"), &mut t).await;
        assert!(t.finish(r).is_err());
        assert_eq!(failed.load(Ordering::SeqCst), 1);

        // Delete: a file, then the uploaded tree (its symlink is not followed).
        #[cfg(unix)]
        std::os::unix::fs::symlink(src.join("sub"), up.join("link")).unwrap();
        assert_eq!(delete(&sftp, &format!("{remote}/src/a.txt")).await.unwrap(), 1);
        assert!(!up.join("a.txt").exists());
        delete(&sftp, &format!("{remote}/src/")).await.unwrap();
        assert!(!up.exists());
        assert!(src.join("sub/b.bin").exists(), "symlink target must survive");
        assert!(delete(&sftp, "/").await.is_err());

        std::fs::remove_dir_all(base).unwrap();
    }
}
