//! Opening a trail: take the lock, then verify every stream before any
//! append can extend one.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use fabric_control_plane::{AuditError, AuditStream};

use super::files::{self, HEAD_EXTENSION, LOG_EXTENSION};
use super::{Heads, Inner, LocalAuditTrail, LocalAuditTrailError};

impl LocalAuditTrail {
    /// Opens the trail in `dir`, creating it if absent.
    ///
    /// Every stream with a log or a head is read and verified. A stream whose
    /// log runs one entry past its head — a stop between the two writes — is
    /// complete, and its head is caught up. A head with no log is a deleted
    /// stream, and is refused like any other truncation.
    ///
    /// # Errors
    ///
    /// [`LocalAuditTrailError`]: the directory or lock is inaccessible,
    /// another process holds it, a file names no stream, or a stream does not
    /// read back or verify.
    pub fn open(dir: &Path) -> Result<Self, LocalAuditTrailError> {
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| LocalAuditTrailError::Io { path, source }
        };
        std::fs::create_dir_all(dir).map_err(io(dir))?;

        let lock_path = dir.join(".audit.lock");
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(io(&lock_path))?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => LocalAuditTrailError::AlreadyOpen {
                path: lock_path.clone(),
            },
            std::fs::TryLockError::Error(source) => LocalAuditTrailError::Io {
                path: lock_path.clone(),
                source,
            },
        })?;

        let mut heads = Heads::default();
        for stream in streams(dir)? {
            let refused = |source| LocalAuditTrailError::Stream {
                path: dir.to_path_buf(),
                source,
            };
            let stored = files::read_stream(dir, &stream).map_err(refused)?;
            let head = stored.verify().map_err(|broken| {
                refused(AuditError::Broken {
                    stream: stream.clone(),
                    broken,
                })
            })?;
            if let Some(head) = head {
                if stored.anchor != Some(head) {
                    files::write_head(dir, &stream, &head).map_err(io(dir))?;
                }
                heads.heads.insert(stream, head);
            }
        }

        Ok(Self {
            inner: Arc::new(Inner {
                dir: dir.to_path_buf(),
                state: Mutex::new(heads),
            }),
            _lock: lock,
        })
    }
}

/// Every stream named by a log or a head in `dir`.
fn streams(dir: &Path) -> Result<BTreeSet<AuditStream>, LocalAuditTrailError> {
    let entries = std::fs::read_dir(dir).map_err(|source| LocalAuditTrailError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut streams = BTreeSet::new();
    for entry in entries {
        let path = entry
            .map_err(|source| LocalAuditTrailError::Io {
                path: dir.to_path_buf(),
                source,
            })?
            .path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        let stream = [LOG_EXTENSION, HEAD_EXTENSION]
            .iter()
            .find_map(|extension| name.strip_suffix(&format!(".{extension}")))
            .and_then(|stem| AuditStream::parse(stem).ok())
            .ok_or_else(|| LocalAuditTrailError::Unrecognised { path: path.clone() })?;
        streams.insert(stream);
    }
    Ok(streams)
}
