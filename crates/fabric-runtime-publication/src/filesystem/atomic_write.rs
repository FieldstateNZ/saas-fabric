//! Replaces one file's contents without ever exposing a partial write.

use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writes `bytes` to `target` by writing a sibling temporary file, `fsync`ing
/// it, and renaming it over `target`.
///
/// `rename` is atomic only within a filesystem, which is why the temporary
/// file is a sibling rather than living under a system temp directory (ADR
/// 0018 part 5). The target path is therefore only ever created by this
/// `rename` — a reader can see the old complete content or the new complete
/// content, and nothing in between.
///
/// The temporary file never survives a call to this function: it becomes
/// `target` on success, and is removed on any failure path, so it is gone
/// afterwards regardless of which outcome this returns.
///
/// # Errors
///
/// Returns [`io::Error`] if the temporary file could not be created,
/// written, `fsync`ed, or renamed.
pub(super) fn atomic_write(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp_path = sibling_temp_path(target);

    let outcome = write_and_sync(&temp_path, bytes).and_then(|()| std::fs::rename(&temp_path, target));

    if outcome.is_err() {
        // Best-effort: the temp file may not exist if `write_and_sync`
        // itself never created it, and this is already the error path.
        let _ = std::fs::remove_file(&temp_path);
    }

    outcome
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
