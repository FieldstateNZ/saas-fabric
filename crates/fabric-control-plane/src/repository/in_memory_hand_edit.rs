//! Storing text the model never validated, the way a hand edit to the
//! desired-state repository does.

use fabric_client_model::{ClientId, ClientRevision};

use super::in_memory::{lock, InMemoryClientRepository};
use super::in_memory_records::ClientRecord;
use super::RepositoryError;

impl InMemoryClientRepository {
    /// Stores `text` as `client`'s document exactly as given, without parsing
    /// or rendering it.
    ///
    /// [`Self::insert`] takes a [`ClientDocument`](fabric_client_model::ClientDocument),
    /// which has already passed validation, so it cannot hold a document the
    /// model refuses. A repository people also edit by hand can, and the read
    /// path has to answer for one: ADR 0019 §4 says a stored document carrying
    /// a shape the model refuses makes the whole client unreadable. This is how
    /// a test puts one there.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if the generated revision could not be
    /// parsed, as [`Self::insert`] does.
    pub fn insert_unvalidated(
        &self,
        client: ClientId,
        text: &str,
    ) -> Result<ClientRevision, RepositoryError> {
        let revision = self.next_revision()?;

        lock(&self.clients).insert(
            client,
            ClientRecord {
                revision: revision.clone(),
                text: text.to_owned(),
            },
        );

        Ok(revision)
    }
}
