//! Replaces one file's contents without ever exposing a partial write.

use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writes `bytes` to `target` by writing a sibling temporary file, `fsync`ing
/// it, renaming it over `target`, and `fsync`ing the directory the rename
/// happened in.
///
/// `rename` is atomic only within a filesystem, which is why the temporary
/// file is a sibling rather than living under a system temp directory (ADR
/// 0018 part 5). The target path is therefore only ever created by this
/// `rename` — a reader can see the old complete content or the new complete
/// content, and nothing in between.
///
/// # Why the directory is `fsync`ed too
///
/// A `rename` is atomic the instant it happens, but on most filesystems the
/// *directory entry* update it makes is not guaranteed durable until the
/// directory itself is `fsync`ed — a crash between the rename and that sync
/// can leave the directory pointing at the old inode again after recovery,
/// even though the new file's own bytes were already synced by
/// [`write_and_sync`]. Opening the parent directory and calling
/// [`std::fs::File::sync_all`] on it is the standard way to make the rename
/// itself, not just the content it points at, survive a crash.
///
/// The temporary file never survives a call to this function: it becomes
/// `target` on success, and is removed on any failure path, so it is gone
/// afterwards regardless of which outcome this returns.
///
/// # Errors
///
/// Returns [`io::Error`] if the temporary file could not be created,
/// written, `fsync`ed, renamed, or if the containing directory could not be
/// opened or `fsync`ed after the rename.
pub(super) fn atomic_write(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp_path = sibling_temp_path(target);

    let outcome = write_and_sync(&temp_path, bytes)
        .and_then(|()| std::fs::rename(&temp_path, target))
        .and_then(|()| sync_directory(target));

    if outcome.is_err() {
        // Best-effort: the temp file may not exist if `write_and_sync`
        // itself never created it, or may already be gone if the rename
        // itself succeeded and only the directory `fsync` afterwards failed
        // — either way, this is already the error path.
        let _ = std::fs::remove_file(&temp_path);
    }

    outcome
}

/// `fsync`s the directory containing `target`, making a preceding `rename`
/// into that directory durable rather than merely atomic.
fn sync_directory(target: &Path) -> io::Result<()> {
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::File::open(directory)?.sync_all()
}

/// Builds the sibling temporary path [`atomic_write`] stages its bytes
/// under, in the same directory as `target` so the later `rename` is atomic.
fn sibling_temp_path(target: &Path) -> PathBuf {
    let file_name = target.file_name().and_then(OsStr::to_str).unwrap_or("document");
    let directory = target.parent().unwrap_or_else(|| Path::new("."));

    directory.join(format!(".{file_name}.tmp"))
}

/// Creates (or truncates) `path`, writes `bytes`, and `fsync`s the result.
fn write_and_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
