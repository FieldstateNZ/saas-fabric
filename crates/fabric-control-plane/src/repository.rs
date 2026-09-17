//! Where desired state is kept, as far as this crate is concerned.

mod binding;
mod change_context;
mod errors;
mod in_memory;
mod in_memory_behaviour;
mod in_memory_documents;
mod in_memory_records;
mod stored_client;
mod unconfigured;

use async_trait::async_trait;
use fabric_client_model::{ClientDocument, ClientId, ClientRevision};

pub use binding::DesiredStateBinding;
pub use change_context::ChangeContext;
pub use errors::RepositoryError;
pub use in_memory::InMemoryClientRepository;
pub use stored_client::StoredClient;
pub use unconfigured::UnconfiguredRepository;

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
/// # `create` exists now; `delete` still does not
///
/// Client creation used to belong to a workflow this crate did not
/// implement. It is now a desired-state write like any other — one document,
/// written once, refused if the id is already taken — so [`create`](Self::create)
/// is a real method here rather than a gap.
///
/// `delete` is still absent, and still deliberately: removing a client is a
/// decision with consequences — application data, secrets, identity sessions
/// — that no single call should be able to take, and there is no
/// deprovisioning workflow yet to take it safely. Adding it later is an
/// additive change; having it here unused would suggest the control plane
/// can already do something it cannot.
///
/// [`create`](Self::create), [`catalogue`](Self::catalogue) and
/// [`save_catalogue`](Self::save_catalogue) all carry a default body that
/// answers [`RepositoryError::NotConfigured`]. That is not laziness: it means
/// an implementation that forgets to override one of them still compiles,
/// and reports itself unconfigured at run time rather than failing to build —
/// the same trade the control plane already makes at its own boundary for a
/// platform nothing has been connected to yet.
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

    /// Creates a client only when its identifier does not already exist.
    /// # Errors
    /// Returns Conflict for a duplicate or an adapter error before changing state.
    async fn create(
        &self,
        _document: &ClientDocument,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        Err(RepositoryError::NotConfigured)
    }

    /// Reads the product catalogue, with no revision before its first write.
    /// # Errors
    /// Returns an adapter error when the configured store cannot be read.
    async fn catalogue(&self) -> Result<fabric_client_model::catalogue::StoredCatalogue, RepositoryError> {
        Err(RepositoryError::NotConfigured)
    }

    /// Replaces the catalogue only if its revision still matches, including absence.
    /// # Errors
    /// Returns Conflict for a stale revision, or an adapter error.
    async fn save_catalogue(
        &self,
        _catalogue: &fabric_client_model::catalogue::Catalogue,
        _expected: Option<&ClientRevision>,
        _change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        Err(RepositoryError::NotConfigured)
    }

    /// A short description for logging, such as a repository name and branch.
    ///
    /// Must not contain a credential.
    fn describe(&self) -> String;
}
