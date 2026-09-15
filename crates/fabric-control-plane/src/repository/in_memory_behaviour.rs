//! What the in-memory repository does when the control plane calls it.
//!
//! Split from the type's own file for the same reason the fake identity
//! provider is: the other file is the surface a *test* drives — seed a client,
//! make the repository unavailable — and this one is the surface the *domain*
//! drives. Two concerns that share a struct, which the house convention puts
//! in two modules rather than one long file.
//!
//! Still in the 121–150 line band even after that split. The reason is the
//! same one `local_repository/operations.rs` gives: `ClientRepository`'s
//! seven methods are one trait impl, which Rust requires in a single
//! `impl` block regardless of how unrelated two of them are to the other
//! five.

use async_trait::async_trait;
use fabric_client_model::catalogue::{Catalogue, StoredCatalogue};
use fabric_client_model::{ClientDocument, ClientId, ClientRevision};

use crate::repository::in_memory::{lock, CatalogueRecord, ClientRecord, InMemoryClientRepository};
use crate::repository::in_memory_documents::{render, stored};
use crate::repository::{ChangeContext, ClientRepository, RepositoryError, StoredClient};

#[async_trait]
impl ClientRepository for InMemoryClientRepository {
    async fn list(&self) -> Result<Vec<StoredClient>, RepositoryError> {
        self.check_available()?;

        lock(&self.clients)
            .iter()
            .map(|(client, record)| stored(client, record))
            .collect()
    }

    async fn get(&self, client: &ClientId) -> Result<StoredClient, RepositoryError> {
        self.check_available()?;

        let clients = lock(&self.clients);
        let record = clients.get(client).ok_or_else(|| RepositoryError::NotFound {
            client: client.clone(),
        })?;

        stored(client, record)
    }

    async fn update(
        &self,
        client: &ClientId,
        document: &ClientDocument,
        expected: &ClientRevision,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        self.check_available()?;

        // The same round trip `create` applies, below: rendered, then
        // parsed back with exactly the code that will read it again, before
        // any lock on the stored map is even taken.
        let text = render(document)?;
        let revision = self.next_revision()?;
        let mut clients = lock(&self.clients);

        let current = clients.get_mut(client).ok_or_else(|| RepositoryError::NotFound {
            client: client.clone(),
        })?;

        if current.revision != *expected {
            return Err(RepositoryError::Conflict);
        }

        current.text = text;
        current.revision = revision.clone();

        Ok(revision)
    }

    async fn create(
        &self,
        document: &ClientDocument,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        self.check_available()?;
        let text = render(document)?;
        let mut clients = lock(&self.clients);
        if clients.contains_key(&document.client().id) {
            return Err(RepositoryError::Conflict);
        }
        let revision = self.next_revision()?;
        clients.insert(
            document.client().id.clone(),
            ClientRecord {
                revision: revision.clone(),
                text,
            },
        );
        Ok(revision)
    }

    async fn catalogue(&self) -> Result<StoredCatalogue, RepositoryError> {
        self.check_available()?;

        let stored = lock(&self.catalogue);
        let Some(record) = stored.as_ref() else {
            return Ok(StoredCatalogue {
                catalogue: Catalogue::default(),
                revision: None,
            });
        };

        // Not `Unavailable`: a catalogue that will not parse is not fixed by
        // asking again, so it is reported as `InvalidCatalogue`, the same
        // failure the Git-backed and local stores report for the same cause.
        let catalogue =
            Catalogue::parse(&record.text).map_err(|source| RepositoryError::InvalidCatalogue { source })?;

        Ok(StoredCatalogue {
            catalogue,
            revision: Some(record.revision.clone()),
        })
    }

    async fn save_catalogue(
        &self,
        catalogue: &Catalogue,
        expected: Option<&ClientRevision>,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        self.check_available()?;
        let text = catalogue.render().map_err(|_| RepositoryError::Rejected {
            detail: "Invalid catalogue".into(),
        })?;
        let mut stored = lock(&self.catalogue);
        if stored.as_ref().map(|record| &record.revision) != expected {
            return Err(RepositoryError::Conflict);
        }
        let revision = self.next_revision()?;
        *stored = Some(CatalogueRecord {
            revision: revision.clone(),
            text,
        });
        Ok(revision)
    }

    fn describe(&self) -> String {
        "in-memory desired state".to_owned()
    }
}
