//! The Git-backed implementation of the desired-state repository.

mod list;
mod write;

use std::sync::Arc;

use async_trait::async_trait;
use fabric_client_model::{ClientDocument, ClientId, ClientRevision};
use fabric_control_plane::{ChangeContext, ClientRepository, RepositoryError, StoredClient};
use fabric_core::Clock;

use crate::github::GitHost;
use crate::{GitCredential, GitRepositoryConfig};

/// Reads and writes client desired state in a Git repository.
pub struct GitClientRepository {
    /// The contents-API client.
    host: GitHost,
}

impl GitClientRepository {
    /// Builds a repository from configuration and a resolved credential.
    ///
    /// # Errors
    ///
    /// Returns a message if the configuration is invalid or the HTTP client
    /// cannot be built.
    pub fn new(
        config: &GitRepositoryConfig,
        credential: GitCredential,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, String> {
        Ok(Self {
            host: GitHost::new(config, credential, clock)?,
        })
    }

    /// Reads and parses one client's document.
    pub(crate) async fn read(&self, client: &ClientId) -> Result<StoredClient, RepositoryError> {
        let file = self.host.read_document(client).await?;

        let document = ClientDocument::parse(&file.text).map_err(|source| RepositoryError::Invalid {
            client: client.clone(),
            source,
        })?;

        Ok(StoredClient {
            document,
            revision: file.revision,
        })
    }
}

#[async_trait]
impl ClientRepository for GitClientRepository {
    async fn list(&self) -> Result<Vec<StoredClient>, RepositoryError> {
        list::list(self, &self.host).await
    }

    async fn get(&self, client: &ClientId) -> Result<StoredClient, RepositoryError> {
        self.read(client).await
    }

    async fn update(
        &self,
        client: &ClientId,
        document: &ClientDocument,
        expected: &ClientRevision,
        change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        write::update(&self.host, client, document, expected, change).await
    }

    fn describe(&self) -> String {
        self.host.describe()
    }
}
