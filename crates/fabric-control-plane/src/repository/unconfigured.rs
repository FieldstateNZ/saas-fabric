//! The repository a control plane has before it has one.
//!
//! # Why this exists rather than an `Option`
//!
//! Desired state is late-bound: the platform starts without knowing where
//! client documents live, and an operator connects it afterwards. The obvious
//! shape for that is `Option<Arc<dyn ClientRepository>>` threaded through the
//! service, the loop and every handler — and every one of those places then
//! has to decide what `None` means, which is how three of them end up
//! deciding differently.
//!
//! So "not configured" is a repository instead. It answers every operation the
//! same way, the rest of the crate has one code path, and the state is
//! impossible to forget to handle because it arrives as an ordinary
//! [`RepositoryError`].

use async_trait::async_trait;
use fabric_client_model::{ClientDocument, ClientId, ClientRevision};

use crate::repository::{ChangeContext, ClientRepository, RepositoryError, StoredClient};

/// A repository that holds nothing and refuses everything, for one reason.
pub struct UnconfiguredRepository;

#[async_trait]
impl ClientRepository for UnconfiguredRepository {
    /// Refused rather than empty.
    ///
    /// An empty list would be a lie an operator cannot see through: a platform
    /// with no clients and a platform that has never been connected to its
    /// desired state look identical, and the second is the one that needs
    /// somebody to act.
    async fn list(&self) -> Result<Vec<StoredClient>, RepositoryError> {
        Err(RepositoryError::NotConfigured)
    }

    async fn get(&self, _client: &ClientId) -> Result<StoredClient, RepositoryError> {
        Err(RepositoryError::NotConfigured)
    }

    async fn update(
        &self,
        _client: &ClientId,
        _document: &ClientDocument,
        _revision: &ClientRevision,
        _context: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        Err(RepositoryError::NotConfigured)
    }

    fn describe(&self) -> String {
        "no desired-state repository".to_owned()
    }
}
