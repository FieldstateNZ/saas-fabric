//! Repository operations serialize writes and publish only committed snapshots.
use super::{unavailable, LocalClientRepository, Record};
use async_trait::async_trait;
use fabric_client_model::{
    catalogue::{Catalogue, StoredCatalogue},
    ClientDocument, ClientId, ClientRevision,
};
use fabric_control_plane::{ChangeContext, ClientRepository, RepositoryError, StoredClient};
#[async_trait]
impl ClientRepository for LocalClientRepository {
    async fn list(&self) -> Result<Vec<StoredClient>, RepositoryError> {
        self.state
            .lock()
            .await
            .clients
            .values()
            .map(|record| record.client().map_err(|_| unavailable("Invalid stored client")))
            .collect()
    }
    async fn get(&self, client: &ClientId) -> Result<StoredClient, RepositoryError> {
        self.state
            .lock()
            .await
            .clients
            .get(client.as_str())
            .ok_or_else(|| RepositoryError::NotFound {
                client: client.clone(),
            })?
            .client()
            .map_err(|_| unavailable("Invalid stored client"))
    }
    async fn update(
        &self,
        client: &ClientId,
        document: &ClientDocument,
        expected: &ClientRevision,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        if &document.client().id != client {
            return Err(RepositoryError::Rejected {
                detail: "Client identifier cannot change".into(),
            });
        }
        let mut stored = self.state.lock().await;
        let record = stored
            .clients
            .get(client.as_str())
            .ok_or_else(|| RepositoryError::NotFound {
                client: client.clone(),
            })?;
        if record.revision != expected.as_str() {
            return Err(RepositoryError::Conflict);
        }
        let mut next = stored.clone();
        let revision = next.next_revision()?;
        next.clients
            .insert(client.to_string(), record_for(document, &revision)?);
        self.commit(&next).await?;
        *stored = next;
        Ok(revision)
    }
    async fn create(
        &self,
        document: &ClientDocument,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        let mut stored = self.state.lock().await;
        let id = document.client().id.as_str();
        if stored.clients.contains_key(id) {
            return Err(RepositoryError::Conflict);
        }
        let mut next = stored.clone();
        let revision = next.next_revision()?;
        next.clients.insert(id.into(), record_for(document, &revision)?);
        self.commit(&next).await?;
        *stored = next;
        Ok(revision)
    }
    async fn catalogue(&self) -> Result<StoredCatalogue, RepositoryError> {
        self.state.lock().await.catalogue()
    }
    async fn save_catalogue(
        &self,
        catalogue: &Catalogue,
        expected: Option<&ClientRevision>,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        let mut stored = self.state.lock().await;
        if stored.catalogue()?.revision.as_ref() != expected {
            return Err(RepositoryError::Conflict);
        }
        let mut next = stored.clone();
        let revision = next.next_revision()?;
        next.catalogue = Some(Record {
            revision: revision.to_string(),
            text: catalogue.render().map_err(|_| unavailable("Invalid catalogue"))?,
        });
        self.commit(&next).await?;
        *stored = next;
        Ok(revision)
    }
    fn describe(&self) -> String {
        "persistent local development desired state".into()
    }
}
fn record_for(document: &ClientDocument, revision: &ClientRevision) -> Result<Record, RepositoryError> {
    Ok(Record {
        revision: revision.to_string(),
        text: document
            .render()
            .map_err(|_| unavailable("Invalid client document"))?,
    })
}
