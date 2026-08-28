//! Where desired state is kept, as far as this crate is concerned.

mod change_context;
mod errors;
mod in_memory;
mod in_memory_behaviour;
mod stored_client;

use async_trait::async_trait;
use fabric_client_model::{ClientDocument, ClientId, ClientRevision};

pub use change_context::ChangeContext;
pub use errors::RepositoryError;
pub use in_memory::InMemoryClientRepository;
pub use stored_client::StoredClient;

/// Reads and writes clients' desired state.
///
/// # What this port hides, and why that matters
///
/// Everything about Git. There is no branch here, no path, no commit, no
/// hosting provider, no HTTP. The domain asks for a client and writes a
/// document at a revision; whether that lands as a commit on `main` in
/// `saas-fabric-clients` or as an entry in a map is the implementation's
/// business.
///
/// That is not abstraction for its own sake. The API's contract is stated in
/// domain terms precisely because the repository's internals must never leak
/// into it — an operator is told "the client changed while you were editing",
/// never "the blob sha of `clients/acme/client.yaml` moved" (specification
/// §8).
///
/// # No `create`, no `delete`
///
/// Both are absent deliberately rather than pending. Creating a client is a
/// workflow this increment does not implement, and deleting one is a decision
/// with consequences no single API call should be able to take. Adding either
/// later is an additive change; having them here unused would suggest the
/// control plane can already do things it cannot.
///
/// # Concurrency is the implementation's job, not the caller's
///
/// [`update`](Self::update) takes the revision the caller believed it was
/// editing, and an implementation **must** refuse the write if that is no
/// longer the current revision. A last-writer-wins repository would satisfy
/// this signature and quietly discard an operator's change; ADR 0008 is
/// explicit that it must not.
#[async_trait]
pub trait ClientRepository: Send + Sync {
    /// Every client the repository holds.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if the repository could not be read. A
    /// repository holding no clients is `Ok(vec![])`; the two are different
    /// answers and must not be conflated.
    async fn list(&self) -> Result<Vec<StoredClient>, RepositoryError>;

    /// One client's desired state, with the revision it is at.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::NotFound`] if no such client exists, or
    /// another variant if the repository could not be read or holds a document
    /// this model cannot understand.
    async fn get(&self, client: &ClientId) -> Result<StoredClient, RepositoryError>;

    /// Replaces a client's document, but only if it is still at `expected`.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Conflict`] if the stored revision has moved
    /// on — the write is then refused entirely, never merged and never
    /// applied on top. Other variants describe a repository that could not be
    /// written to at all.
    async fn update(
        &self,
        client: &ClientId,
        document: &ClientDocument,
        expected: &ClientRevision,
        change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError>;

    /// A short description for logging, such as a repository name and branch.
    ///
    /// Must not contain a credential.
    fn describe(&self) -> String;
}
