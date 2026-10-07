//! Replaces one file's contents without ever exposing a partial write.

use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writes `bytes` to `target` by writing a sibling temporary file, `fsync`ing
/// it, renaming it over `target`, and, on Unix, `fsync`ing the directory the
/// rename happened in.
///
/// `rename` is atomic only within a filesystem, which is why the temporary
/// file is a sibling rather than living under a system temp directory (ADR
/// 0018 part 5). The target path is therefore only ever created by this
/// `rename` — a reader can see the old complete content or the new complete
/// content, and nothing in between, on every platform.
///
/// # Why the directory is `fsync`ed too, and only on Unix
///
/// A `rename` is atomic the instant it happens, but on most filesystems the
/// *directory entry* update it makes is not guaranteed durable until the
/// directory itself is `fsync`ed — a crash between the rename and that sync
/// can leave the directory pointing at the old inode again after recovery,
/// even though the new file's own bytes were already synced by
/// [`write_and_sync`]. Opening the parent directory and calling
/// [`std::fs::File::sync_all`] on it is the standard way to make the rename
/// itself, not just the content it points at, survive a crash. See
/// [`sync_directory`] for why this is a no-op off Unix.
///
/// # Why the temporary file is created exclusively
///
/// Every write of `target` stages through the same sibling path. Created
/// with a truncating open, a second writer staging the same target emptied
/// the first writer's staged bytes, or wrote into the inode the first had
/// just renamed into place, and the published document came out empty or
/// not JSON (gap G4a in `docs/roadmap/m2-publication-gap-report.md`). The
/// temporary file is therefore opened with `create_new`: a second writer
/// that finds one already there fails with `AlreadyExists` and touches
/// nothing. The adapter's publication lock means no second writer gets this
/// far through `publish`; this keeps `atomic_write` safe on its own terms,
/// and [`remove_stale_staging`] clears what a crashed writer left.
///
/// A temporary file this call created never survives it: it becomes `target`
/// on success, and is removed on any failure path. One it did not create --
/// another writer's -- is never removed here.
///
/// # Errors
///
/// Returns [`io::Error`] if the temporary file could not be created,
/// written, `fsync`ed, renamed, or if the containing directory could not be
/// opened or `fsync`ed after the rename.
pub(super) fn atomic_write(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp_path = sibling_temp_path(target);

    if let Err(error) = write_and_sync(&temp_path, bytes) {
        // `AlreadyExists` means the file at `temp_path` is not this call's:
        // another writer is staging `target`, or a crashed one left it.
        if error.kind() != io::ErrorKind::AlreadyExists {
            let _ = std::fs::remove_file(&temp_path);
        }
        return Err(error);
    }

    if let Err(error) = std::fs::rename(&temp_path, target) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }

    // After the rename the temporary path is no longer this call's, so a
    // failed directory `fsync` removes nothing: another writer may already
    // be staging there.
    sync_directory(target)
}

/// `fsync`s the directory containing `target`, making a preceding `rename`
/// into that directory durable rather than merely atomic.
///
/// Unix only: opening a directory with [`std::fs::File::open`] is not
/// portable, and fails outright on Windows. The non-Unix version below is a
/// documented no-op, so the rename itself stays atomic everywhere but this
/// extra durability against a crash between the rename and the next `fsync`
/// is Unix only — see `docs/README.md`.
#[cfg(unix)]
fn sync_directory(target: &Path) -> io::Result<()> {
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::File::open(directory)?.sync_all()
}

/// The non-Unix half of [`sync_directory`] above: a no-op, since there is no
/// portable way to `fsync` a directory outside Unix. The rename this follows
/// is still atomic; only the extra durability is unavailable here.
#[cfg(not(unix))]
fn sync_directory(_target: &Path) -> io::Result<()> {
    Ok(())
}

/// Builds the sibling temporary path [`atomic_write`] stages its bytes
/// under, in the same directory as `target` so the later `rename` is atomic.
fn sibling_temp_path(target: &Path) -> PathBuf {
    let file_name = target.file_name().and_then(OsStr::to_str).unwrap_or("document");
    let directory = target.parent().unwrap_or_else(|| Path::new("."));

    directory.join(format!(".{file_name}.tmp"))
}

/// Removes the temporary file a writer of `target` left behind, if any.
///
/// Called only while the publication lock is held, when no writer that goes
/// through the adapter can be staging `target`: whatever is there was left
/// by a writer that crashed between creating it and renaming it, and would
/// otherwise refuse every later write of `target` with `AlreadyExists`.
/// Best-effort: anything that cannot be removed, such as a directory, is
/// left for [`atomic_write`] to fail on and report.
pub(super) fn remove_stale_staging(target: &Path) {
    let _ = std::fs::remove_file(sibling_temp_path(target));
}

/// Creates `path`, which must not already exist, writes `bytes`, and
/// `fsync`s the result.
fn write_and_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
#[path = "atomic_write_tests.rs"]
mod tests;
