//! Repository operations serialize writes and publish only committed snapshots.
//!
//! In the 121–150 line band. The reason is that Rust requires every method
//! of a trait in one `impl` block — `ClientRepository`'s seven methods
//! cannot be spread across files the way free functions can — together
//! with the two small helpers (`render_client`, `invalid`) those methods
//! share and that have no other caller to be useful to.
use super::commit::commit_and_swap;
use super::error_helpers::{rejected, unavailable};
use super::{LocalClientRepository, Record};
use async_trait::async_trait;
use fabric_client_model::{
    catalogue::{Catalogue, StoredCatalogue},
    ClientDocument, ClientId, ClientRevision, DesiredStateError,
};
use fabric_control_plane::{ChangeContext, ClientRepository, RepositoryError, StoredClient};
use std::sync::Arc;
#[async_trait]
impl ClientRepository for LocalClientRepository {
    async fn list(&self) -> Result<Vec<StoredClient>, RepositoryError> {
        self.inner
            .state
            .lock()
            .await
            .clients
            .iter()
            .map(|(id, record)| record.client().map_err(|source| invalid(id, source)))
            .collect()
    }
    async fn get(&self, client: &ClientId) -> Result<StoredClient, RepositoryError> {
        let stored = self.inner.state.lock().await;
        let record = stored
            .clients
            .get(client.as_str())
            .ok_or_else(|| RepositoryError::NotFound {
                client: client.clone(),
            })?;
        record.client().map_err(|source| RepositoryError::Invalid {
            client: client.clone(),
            source,
        })
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
        let text = render_client(document)?;
        let key = client.to_string();
        let not_found = client.clone();
        let expected = expected.as_str().to_owned();

        commit_and_swap(Arc::clone(&self.inner), move |next| {
            let current = next.clients.get(&key).ok_or_else(|| RepositoryError::NotFound {
                client: not_found.clone(),
            })?;
            if current.revision != expected {
                return Err(RepositoryError::Conflict);
            }
            let revision = next.next_revision()?;
            next.clients.insert(
                key.clone(),
                Record {
                    revision: revision.to_string(),
                    text,
                },
            );
            Ok(revision)
        })
        .await
    }
    async fn create(
        &self,
        document: &ClientDocument,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        let text = render_client(document)?;
        let key = document.client().id.to_string();

        commit_and_swap(Arc::clone(&self.inner), move |next| {
            if next.clients.contains_key(&key) {
                return Err(RepositoryError::Conflict);
            }
            let revision = next.next_revision()?;
            next.clients.insert(
                key,
                Record {
                    revision: revision.to_string(),
                    text,
                },
            );
            Ok(revision)
        })
        .await
    }
    async fn catalogue(&self) -> Result<StoredCatalogue, RepositoryError> {
        self.inner.state.lock().await.catalogue()
    }
    async fn save_catalogue(
        &self,
        catalogue: &Catalogue,
        expected: Option<&ClientRevision>,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        let text = catalogue.render().map_err(|_| rejected("Invalid catalogue"))?;
        let expected = expected.cloned();

        commit_and_swap(Arc::clone(&self.inner), move |next| {
            if next.catalogue()?.revision != expected {
                return Err(RepositoryError::Conflict);
            }
            let revision = next.next_revision()?;
            next.catalogue = Some(Record {
                revision: revision.to_string(),
                text,
            });
            Ok(revision)
        })
        .await
    }
    fn describe(&self) -> String {
        "persistent local development desired state".into()
    }
}
/// Renders a client document, mapping a failure the way `save_catalogue` maps
/// a catalogue's — [`rejected`], not [`unavailable`]: a document that will
/// not render is invalid, not a repository having a bad minute.
fn render_client(document: &ClientDocument) -> Result<String, RepositoryError> {
    document.render().map_err(|_| rejected("Invalid client document"))
}
/// Builds the "stored document will not parse" error for a listing, or falls
/// back to `Unavailable` in the one case that should be unreachable: the
/// snapshot's own key is not a valid client id, even though `open` checks
/// every key against its document before this store is ever handed out.
fn invalid(id: &str, source: DesiredStateError) -> RepositoryError {
    match ClientId::try_new(id) {
        Ok(client) => RepositoryError::Invalid { client, source },
        Err(_) => unavailable("a stored client key is not a valid client id"),
    }
}
