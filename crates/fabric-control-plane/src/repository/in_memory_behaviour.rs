//! What the in-memory repository does when the control plane calls it.
//!
//! Split from the type's own file for the same reason the fake identity
//! provider is: the other file is the surface a *test* drives — seed a client,
//! make the repository unavailable — and this one is the surface the *domain*
//! drives. Two concerns that share a struct, which the house convention puts
//! in two modules rather than one long file.

use async_trait::async_trait;
use fabric_client_model::{ClientDocument, ClientId, ClientRevision};

use crate::repository::in_memory::{lock, InMemoryClientRepository};
use crate::repository::{ChangeContext, ClientRepository, RepositoryError, StoredClient};

#[async_trait]
impl ClientRepository for InMemoryClientRepository {
    async fn list(&self) -> Result<Vec<StoredClient>, RepositoryError> {
        self.check_available()?;

        Ok(lock(&self.clients).values().cloned().collect())
    }

    async fn get(&self, client: &ClientId) -> Result<StoredClient, RepositoryError> {
        self.check_available()?;

        lock(&self.clients)
            .get(client)
            .cloned()
            .ok_or_else(|| RepositoryError::NotFound {
                client: client.clone(),
            })
    }

    async fn update(
        &self,
        client: &ClientId,
        document: &ClientDocument,
        expected: &ClientRevision,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        self.check_available()?;

        let revision = self.next_revision()?;
        let mut clients = lock(&self.clients);

        let current = clients.get_mut(client).ok_or_else(|| RepositoryError::NotFound {
            client: client.clone(),
        })?;

        if current.revision != *expected {
            return Err(RepositoryError::Conflict);
        }

        current.document = document.clone();
        current.revision = revision.clone();

        Ok(revision)
    }

    fn describe(&self) -> String {
        "in-memory desired state".to_owned()
    }
}
