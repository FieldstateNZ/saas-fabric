//! Durable single-process development storage with atomic snapshot replacement.
mod operations;
mod storage;
use fabric_client_model::{
    catalogue::{Catalogue, StoredCatalogue},
    ClientDocument, ClientRevision,
};
use fabric_control_plane::{RepositoryError, StoredClient};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use tokio::sync::Mutex;

/// Local development repository. An OS lock prevents two processes opening it.
pub struct LocalClientRepository {
    state: Mutex<Snapshot>,
    path: PathBuf,
    _lock: std::fs::File,
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
impl LocalClientRepository {
    /// Opens a persistent development store, importing YAML clients on first use.
    /// # Errors
    /// Refuses invalid snapshots, inaccessible directories and concurrent processes.
    pub async fn open(path: &Path) -> Result<Self, String> {
        tokio::fs::create_dir_all(path).await.map_err(|e| e.to_string())?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join(".fabric-state.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|_| "Another process has this local repository open".to_owned())?;
        let state_path = path.join(".fabric-state.json");
        let snapshot = match tokio::fs::read_to_string(&state_path).await {
            Ok(text) => serde_json::from_str::<Snapshot>(&text).map_err(|e| e.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => storage::import(path).await?,
            Err(error) => return Err(error.to_string()),
        };
        for (id, record) in &snapshot.clients {
            let stored = record.client()?;
            if stored.document.client().id.as_str() != id {
                return Err("Stored client key does not match its document".into());
            }
        }
        if let Some(record) = &snapshot.catalogue {
            Catalogue::parse(&record.text).map_err(|e| e.to_string())?;
            record.revision()?;
        }
        Ok(Self {
            state: Mutex::new(snapshot),
            path: state_path,
            _lock: lock,
        })
    }
    async fn commit(&self, next: &Snapshot) -> Result<(), RepositoryError> {
        storage::commit(&self.path, next).await
    }
}
impl Record {
    fn revision(&self) -> Result<ClientRevision, String> {
        ClientRevision::try_new(&self.revision).map_err(|e| e.to_string())
    }
    fn client(&self) -> Result<StoredClient, String> {
        Ok(StoredClient {
            document: ClientDocument::parse(&self.text).map_err(|e| e.to_string())?,
            revision: self.revision()?,
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
                catalogue: Catalogue::parse(&record.text).map_err(|_| unavailable("Invalid catalogue"))?,
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
