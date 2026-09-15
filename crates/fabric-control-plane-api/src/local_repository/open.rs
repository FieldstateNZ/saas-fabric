//! Opening a store: acquire the OS lock, load or import a snapshot, and
//! validate every document already on disk once — so a later read never has
//! to discover a stored document does not parse.
use super::{catalogue_error, invalid_snapshot, io, storage, Inner, LocalClientRepository, Snapshot};
use crate::local_repository::LocalRepositoryError;
use fabric_client_model::catalogue::Catalogue;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

impl LocalClientRepository {
    /// Opens a persistent development store, importing YAML clients on first use.
    /// # Errors
    /// Refuses invalid snapshots, inaccessible directories and concurrent processes.
    pub async fn open(path: &Path) -> Result<Self, LocalRepositoryError> {
        tokio::fs::create_dir_all(path)
            .await
            .map_err(|source| io(path, source))?;

        let lock_path = path.join(".fabric-state.lock");
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| io(&lock_path, source))?;

        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => LocalRepositoryError::AlreadyOpen {
                path: lock_path.clone(),
            },
            std::fs::TryLockError::Error(source) => io(&lock_path, source),
        })?;

        let state_path = path.join(".fabric-state.json");
        let snapshot = load(path, &state_path).await?;
        validate(&snapshot, &state_path)?;

        Ok(Self {
            inner: Arc::new(Inner {
                state: Mutex::new(snapshot),
                path: state_path,
            }),
            _lock: lock,
        })
    }
}

/// Reads the snapshot at `state_path`, importing top-level YAML documents
/// under `path` on its first open, when no snapshot exists yet.
async fn load(path: &Path, state_path: &Path) -> Result<Snapshot, LocalRepositoryError> {
    match tokio::fs::read_to_string(state_path).await {
        Ok(text) => {
            serde_json::from_str::<Snapshot>(&text).map_err(|error| invalid_snapshot(state_path, &error))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            storage::import(path)
                .await
                .map_err(|detail| LocalRepositoryError::InvalidSnapshot {
                    path: path.to_path_buf(),
                    detail,
                })
        }
        Err(error) => Err(io(state_path, error)),
    }
}

/// Checks every document a loaded snapshot already holds, once at open, so a
/// later `get` or `list` never has to discover a stored document does not
/// parse.
fn validate(snapshot: &Snapshot, state_path: &Path) -> Result<(), LocalRepositoryError> {
    for (id, record) in &snapshot.clients {
        let stored = record
            .client()
            .map_err(|source| LocalRepositoryError::InvalidClient {
                path: state_path.to_path_buf(),
                source,
            })?;
        if stored.document.client().id.as_str() != id {
            return Err(LocalRepositoryError::InvalidSnapshot {
                path: state_path.to_path_buf(),
                detail: "a stored client key does not match its document".into(),
            });
        }
    }

    if let Some(record) = &snapshot.catalogue {
        Catalogue::parse(&record.text).map_err(|source| catalogue_error(state_path, source))?;
        record
            .revision()
            .map_err(|_| LocalRepositoryError::InvalidSnapshot {
                path: state_path.to_path_buf(),
                detail: "the stored catalogue revision is invalid".into(),
            })?;
    }

    Ok(())
}
