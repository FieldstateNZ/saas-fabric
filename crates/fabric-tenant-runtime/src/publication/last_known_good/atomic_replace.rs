//! Replacing the last-known-good file without ever exposing a partial one.
//!
//! The same discipline as the publisher's own `atomic_write`, which the
//! runtime plane may not link (ADR 0018): stage a sibling file created with
//! `create_new` under a name unique to this write, fill it, `fsync` it,
//! rename it over the target, then `fsync` the directory. A reader of the
//! target sees the previous complete file or the new complete file.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::AsyncWriteExt as _;

/// The points at which a write can stop, named so a test can stop it at
/// each one and show the target is still whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// The staging file exists and holds the bytes, not yet synced.
    Written,
    /// The staging file is synced, not yet renamed.
    Synced,
    /// The rename happened; the directory is not yet synced.
    Renamed,
}

/// The prefix every staging file of `target` carries, so a crashed write's
/// leftovers can be found and cleared.
pub(crate) fn staging_prefix(target: &Path) -> String {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    format!(".{name}.")
}

/// Writes `bytes` over `target`, calling `checkpoint` after each [`Step`].
/// A checkpoint error stops the write there, as a crash or I/O failure
/// would; production passes one that always continues.
///
/// A staging file this call created is removed on any failure before the
/// rename. One left by a process that died mid-write is never touched here:
/// see [`clear_stale_staging`].
///
/// # Errors
///
/// [`io::Error`] from any step, or from `checkpoint`.
pub(crate) async fn atomic_replace(
    target: &Path,
    bytes: &[u8],
    checkpoint: impl Fn(Step) -> io::Result<()>,
) -> io::Result<()> {
    let staging = staging_path(target);
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .await?;
    let staged = async {
        file.write_all(bytes).await?;
        checkpoint(Step::Written)?;
        file.sync_all().await?;
        checkpoint(Step::Synced)?;
        tokio::fs::rename(&staging, target).await
    }
    .await;
    if let Err(error) = staged {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(error);
    }
    checkpoint(Step::Renamed)?;
    sync_directory(target).await
}

/// Removes staging files a crashed writer left beside `target`.
///
/// Called once, when a reader starts and before it writes anything. The
/// directory is expected to be this replica's own (an `emptyDir` or a
/// per-replica volume); a write another process has in flight in a shared
/// one would fail its rename and be reported, never corrupt the target.
pub(crate) async fn clear_stale_staging(target: &Path) {
    let (Some(directory), prefix) = (target.parent(), staging_prefix(target)) else {
        return;
    };
    let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".tmp") {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

fn staging_path(target: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let unique = format!(
        "{}{}.{}.tmp",
        staging_prefix(target),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    target.with_file_name(unique)
}

/// `fsync`s the directory holding `target`, so a rename into or out of it
/// survives a crash, not only happens atomically.
#[cfg(unix)]
pub(super) async fn sync_directory(target: &Path) -> io::Result<()> {
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    tokio::fs::File::open(directory).await?.sync_all().await
}

/// No portable directory `fsync` outside Unix; the rename is still atomic.
#[cfg(not(unix))]
pub(super) async fn sync_directory(_target: &Path) -> io::Result<()> {
    Ok(())
}
