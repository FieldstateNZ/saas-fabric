//! The one-publisher-at-a-time lock the filesystem adapter takes around a
//! publication.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use super::paths::DocumentPaths;
use crate::PublicationError;

/// Held for the whole of one publication: from reading what is held,
/// through planning, to the last write. Released when dropped.
///
/// # Why
///
/// `publish` reads the held documents, plans against them, then writes.
/// Two publications that overlap both plan against the same held state, and
/// each writes only the documents its own plan changes. Each can be valid
/// against what it read while the two together publish a binding to a
/// DataSource that is gone, or replace one another's payload at the same
/// revision (gap G4 in `docs/roadmap/m2-publication-gap-report.md`). They
/// would also stage the same document through the same sibling temporary
/// path. ADR 0018 assumes exactly one writer; this enforces it for every
/// publisher that goes through this adapter.
///
/// # Refused, not waited for
///
/// A second publication is refused at once with `Unwritable` rather than
/// blocked: this adapter runs synchronous I/O inside an `async fn` (see
/// `filesystem.rs`), and a lock wait would hold an executor thread for as
/// long as the other publication runs. Nothing has been written when it is
/// refused, and the controller re-offers at once and on its next pass.
///
/// # What it does not cover
///
/// It is an advisory lock (`flock` on Unix, `LockFileEx` on Windows): a
/// process that writes the files without taking it is not stopped, and on a
/// network filesystem whether it holds across hosts depends on that
/// filesystem.
pub(super) struct PublicationLock {
    _file: File,
}

impl PublicationLock {
    /// Takes the lock that sits beside the tenants payload, the document
    /// every publication writes last.
    ///
    /// # Errors
    ///
    /// [`PublicationError::Unwritable`] for the tenants document if another
    /// publication holds the lock, or if the lock file cannot be opened.
    pub(super) fn acquire(tenants: &DocumentPaths) -> Result<Self, PublicationError> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(lock_path(&tenants.payload))
            .map_err(|cause| unwritable(tenants, cause))?;

        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(unwritable(tenants, PublicationInProgress)),
            Err(TryLockError::Error(cause)) => Err(unwritable(tenants, cause)),
        }
    }
}

/// `.{tenants file}.lock`, beside it, so every adapter configured with the
/// same tenants path shares one lock.
fn lock_path(tenants: &Path) -> PathBuf {
    let file_name = tenants.file_name().and_then(OsStr::to_str).unwrap_or("document");
    let directory = tenants.parent().unwrap_or_else(|| Path::new("."));

    directory.join(format!(".{file_name}.lock"))
}

/// Another publication through this adapter is still running.
#[derive(Debug, thiserror::Error)]
#[error("another publication is in progress; nothing was written")]
struct PublicationInProgress;

fn unwritable(
    tenants: &DocumentPaths,
    cause: impl std::error::Error + Send + Sync + 'static,
) -> PublicationError {
    PublicationError::Unwritable {
        document: tenants.kind,
        cause: Box::new(cause),
    }
}
