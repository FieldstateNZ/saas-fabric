//! Making the trail's writes reach storage: a file's contents, and the
//! directory entry that names it.
//!
//! Syncing a file does not make its name durable; the directory holding it
//! needs a sync of its own (fsync(2)). So a stream's first entry is not
//! appended until its directory is synced, and a head is not replaced until
//! the rename is.

use std::fs::File;
use std::io;
use std::path::Path;

/// How the trail syncs. [`Os`] in every real trail; a test substitutes one
/// that fails on cue, which no real disk can be made to do.
pub(super) trait Durability: Send + Sync {
    /// Flushes `file`, found at `path`, to storage.
    fn sync_file(&self, file: &File, path: &Path) -> io::Result<()>;

    /// Flushes `dir`'s entries to storage.
    fn sync_dir(&self, dir: &Path) -> io::Result<()>;
}

/// The operating system's own syncs.
pub(super) struct Os;

impl Durability for Os {
    fn sync_file(&self, file: &File, _path: &Path) -> io::Result<()> {
        file.sync_all()
    }

    #[cfg(unix)]
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        File::open(dir)?.sync_all()
    }

    // Windows has no directory handle `std` can sync; NTFS journals the
    // entry with the file. CI and every deployment are Unix.
    #[cfg(not(unix))]
    fn sync_dir(&self, _dir: &Path) -> io::Result<()> {
        Ok(())
    }
}

/// Creates `dir` and any missing ancestors, then syncs the parent of every
/// component of `dir`, existing or not, so each name the trail depends on is
/// durable. Syncing all of them, not only those this call created, is what
/// makes a retry safe: a directory created by an earlier attempt whose sync
/// failed exists now, and is synced all the same.
pub(super) fn create_dir(dir: &Path, durability: &dyn Durability) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut components: Vec<&Path> = dir
        .ancestors()
        .filter(|ancestor| !ancestor.as_os_str().is_empty())
        .collect();
    components.reverse();
    for component in components {
        match component.parent() {
            // A relative path's first component lives in the working directory.
            Some(parent) if parent.as_os_str().is_empty() => durability.sync_dir(Path::new("."))?,
            Some(parent) => durability.sync_dir(parent)?,
            None => {}
        }
    }
    Ok(())
}
