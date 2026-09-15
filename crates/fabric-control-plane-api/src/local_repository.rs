//! Durable single-process development storage with atomic snapshot replacement.
mod commit;
mod errors;
mod open;
mod operations;
mod storage;
use fabric_client_model::{
    catalogue::{Catalogue, StoredCatalogue},
    ClientDocument, ClientRevision, DesiredStateError,
};
use fabric_control_plane::{RepositoryError, StoredClient};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::Mutex;

pub use errors::LocalRepositoryError;

/// Local development repository. An OS lock prevents two processes opening it.
///
/// # Why the state is behind an `Arc`, not held directly
///
/// A write here is commit-then-swap: render, write the whole snapshot to
/// disk, and only then update the in-memory copy. Doing that against `&self`
/// directly ties the write's lifetime to the request that asked for it — and
/// a request can be cancelled at any `.await`, including one in the middle
/// of a commit already landing on disk. A future that is dropped mid-write
/// does not get to finish; it stops exactly where it was suspended, which is
/// how a commit already on disk could be abandoned before the in-memory copy
/// is told about it — disk ahead of memory, and every read after that stale
/// until the next write happens to fix it by accident. Every write below
/// instead hands its work to a spawned task, over a clone of this `Arc`, so
/// the commit-and-swap runs to completion whether or not the caller is still
/// waiting on it — the same shape `docs/architecture/control-plane.md`
/// describes for a transition a handler only awaits.
pub struct LocalClientRepository {
    inner: Arc<Inner>,
    _lock: std::fs::File,
}
/// The state a write locks, clones, mutates and — if the commit succeeds —
/// swaps back in.
struct Inner {
    state: Mutex<Snapshot>,
    path: PathBuf,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Snapshot {
    writes: u64,
    clients: BTreeMap<String, Record>,
    catalogue: Option<Record>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    revision: String,
    text: String,
}
impl Record {
    fn revision(&self) -> Result<ClientRevision, String> {
        ClientRevision::try_new(&self.revision).map_err(|e| e.to_string())
    }
    fn client(&self) -> Result<StoredClient, DesiredStateError> {
        Ok(StoredClient {
            document: ClientDocument::parse(&self.text)?,
            revision: self.revision().map_err(|_| DesiredStateError::Malformed {
                detail: "invalid stored revision".into(),
            })?,
        })
    }
}
impl Snapshot {
    fn next_revision(&mut self) -> Result<ClientRevision, RepositoryError> {
        self.writes = self
            .writes
            .checked_add(1)
            .ok_or_else(|| unavailable("Revision limit reached"))?;
        ClientRevision::try_new(format!("local-{}", self.writes))
            .map_err(|_| unavailable("Invalid local revision"))
    }
    fn catalogue(&self) -> Result<StoredCatalogue, RepositoryError> {
        match &self.catalogue {
            None => Ok(StoredCatalogue {
                catalogue: Catalogue::default(),
                revision: None,
            }),
            Some(record) => Ok(StoredCatalogue {
                // Not `unavailable`: retrying reads the same broken text
                // again, so this carries the parse failure as
                // `InvalidCatalogue` instead.
                catalogue: Catalogue::parse(&record.text)
                    .map_err(|source| RepositoryError::InvalidCatalogue { source })?,
                revision: Some(
                    record
                        .revision()
                        .map_err(|_| unavailable("Invalid catalogue revision"))?,
                ),
            }),
        }
    }
}
fn unavailable(detail: &str) -> RepositoryError {
    RepositoryError::Unavailable {
        detail: detail.into(),
    }
}
/// Wraps an I/O failure with the path it happened against.
fn io(path: &Path, source: std::io::Error) -> LocalRepositoryError {
    LocalRepositoryError::Io {
        path: path.to_path_buf(),
        source,
    }
}
/// Wraps a snapshot JSON failure with the path it came from.
fn invalid_snapshot(path: &Path, error: &serde_json::Error) -> LocalRepositoryError {
    LocalRepositoryError::InvalidSnapshot {
        path: path.to_path_buf(),
        detail: error.to_string(),
    }
}
/// Distinguishes a pre-envelope catalogue from every other way one can fail
/// to parse — see [`LocalRepositoryError::LegacyCatalogue`].
fn catalogue_error(path: &Path, source: DesiredStateError) -> LocalRepositoryError {
    if matches!(source, DesiredStateError::UnknownDocumentKind { .. }) {
        LocalRepositoryError::LegacyCatalogue {
            path: path.to_path_buf(),
            source,
        }
    } else {
        LocalRepositoryError::InvalidCatalogue {
            path: path.to_path_buf(),
            source,
        }
    }
}
