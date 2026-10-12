//! The audit log file: one JSON object per line, each appended and synced
//! through the one handle the identity checks were made on.

use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use super::identity::{identity, Identity};

/// The audit file: opened and checked now if it exists, or created at the
/// first line, so a restore refused before its intent leaves no file.
pub(super) struct AuditFile {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl AuditFile {
    /// Checks `path` without writing anything: its directory holds no
    /// published file; it is not a symbolic link; and an existing file is a
    /// regular file that is neither the source nor any published file, and
    /// is the file this handle opened.
    pub(super) fn open(
        path: &Path,
        published_directories: &[PathBuf],
        forbidden: &[Identity],
    ) -> Result<Self, String> {
        let directory = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()
            .map_err(|error| format!("the audit log's directory cannot be used: {error}"))?;
        if published_directories.iter().any(|published| {
            published
                .canonicalize()
                .is_ok_and(|published| published == directory)
        }) {
            return Err("the audit log may not be in a published document's directory".to_owned());
        }
        let file = match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("the audit log cannot be examined: {error}")),
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("the audit log may not be a symbolic link".to_owned())
            }
            Ok(metadata) if !metadata.is_file() => {
                return Err("the audit log is not a regular file".to_owned())
            }
            Ok(metadata) => {
                let file = OpenOptions::new()
                    .append(true)
                    .open(path)
                    .map_err(|error| format!("the audit log cannot be opened: {error}"))?;
                let opened = file
                    .metadata()
                    .map_err(|error| format!("the audit log cannot be examined: {error}"))?;
                if identity(&opened) != identity(&metadata) {
                    return Err("the audit log changed while it was opened".to_owned());
                }
                if forbidden.contains(&identity(&opened)) {
                    return Err(
                        "the audit log is the source or a published file; give one apart from both"
                            .to_owned(),
                    );
                }
                Some(file)
            }
        };
        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(file),
        })
    }

    /// Appends one line and syncs it; creates the file first if needed,
    /// never through a link, and syncs its directory so the file survives.
    pub(super) fn append(&self, record: &Value) -> io::Result<()> {
        let mut line = serde_json::to_vec(record).map_err(io::Error::other)?;
        line.push(b'\n');
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("the audit log's lock is poisoned"))?;
        if file.is_none() {
            let created = OpenOptions::new()
                .append(true)
                .create_new(true)
                .open(&self.path)?;
            created.sync_all()?;
            sync_directory(&self.path)?;
            *file = Some(created);
        }
        let file = file
            .as_mut()
            .ok_or_else(|| io::Error::other("the audit log is not open"))?;
        file.write_all(&line)?;
        file.sync_all()
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}
