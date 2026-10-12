//! What a hand edit to the desired-state repository can do that the port
//! cannot: store text the model never validated, or remove a document.

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

    /// Removes `client`'s document outright, the way a commit deleting
    /// `clients/<id>/client.yaml` does: a revert of the commit that created
    /// it, say. [`ClientRepository`](crate::ClientRepository) has no removal
    /// on purpose, so this is how a test puts a repository in that state.
    ///
    /// Returns whether a document was there to remove.
    #[must_use]
    pub fn remove_by_hand(&self, client: &ClientId) -> bool {
        lock(&self.clients).remove(client).is_some()
    }
}
