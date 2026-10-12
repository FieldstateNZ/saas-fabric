//! Making the trail's writes reach storage: a file's contents, and the
//! directory entry that names it.
//!
//! Syncing a file does not make its name durable; the directory holding it
//! needs a sync of its own (fsync(2)). So a stream's first entry is not
//! appended until its directory is synced, and a head is not replaced until
//! the rename is.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

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

/// Creates `dir` and any missing ancestors, syncing the parent of each one
/// created so the new names survive a crash.
pub(super) fn create_dir(dir: &Path, durability: &dyn Durability) -> io::Result<()> {
    let created: Vec<PathBuf> = dir
        .ancestors()
        .take_while(|ancestor| !ancestor.as_os_str().is_empty() && !ancestor.exists())
        .map(Path::to_path_buf)
        .collect();
    std::fs::create_dir_all(dir)?;
    for path in created.iter().rev() {
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            durability.sync_dir(parent)?;
        }
    }
    Ok(())
}
