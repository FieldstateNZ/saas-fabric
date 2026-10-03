//! Product catalogue operations over the current desired-state binding.
//!
//! In the 121–150 line band: a write's read, its check and its save are one
//! sequence, and `CatalogueRead` carries the repository from one to the other.
use std::sync::Arc;

use crate::{audit, ChangeContext, ClientRepository, ClientService, ControlPlaneError, Operator};
use fabric_client_model::{
    catalogue::{Catalogue, CatalogueCommand, StoredCatalogue},
    ClientRevision,
};
/// A catalogue read for a write, and the repository it was read from.
///
/// # Why the repository travels with it
///
/// The binding answers "the repository to use for this operation", and a
/// rebind can land between a read and its write — likelier across a
/// selection's registry reads. A write sent to whatever the binding names by
/// then could land in a repository other than the one whose revision it was
/// checked against, so the write goes where the read came from.
pub(crate) struct CatalogueRead {
    /// Where it was read, and where the write goes.
    repository: Arc<dyn ClientRepository>,

    /// What was read.
    pub(crate) stored: StoredCatalogue,
}

impl ClientService {
    /// Reads the current product catalogue.
    /// # Errors
    /// Reports repository failures without returning upstream details.
    pub async fn catalogue(&self) -> Result<StoredCatalogue, ControlPlaneError> {
        self.repository
            .current()
            .catalogue()
            .await
            .map_err(ControlPlaneError::from_repository)
    }
    /// Reads the catalogue a write is about to change, refusing it unless it
    /// is still at `expected`.
    /// # Errors
    /// [`ControlPlaneError::CatalogueRevisionConflict`] for a revision this
    /// request never saw, and repository failures.
    pub(crate) async fn catalogue_at(
        &self,
        expected: Option<&ClientRevision>,
    ) -> Result<CatalogueRead, ControlPlaneError> {
        let repository = self.repository.current();
        let stored = repository
            .catalogue()
            .await
            .map_err(ControlPlaneError::from_repository)?;
        if stored.revision.as_ref() != expected {
            return Err(ControlPlaneError::CatalogueRevisionConflict);
        }
        Ok(CatalogueRead { repository, stored })
    }
    /// Applies one catalogue command with optimistic concurrency.
    /// # Errors
    /// Rejects stale edits and invalid or inconsistent application definitions.
    pub async fn change_catalogue(
        &self,
        operator: &Operator,
        command: CatalogueCommand,
        expected: Option<&ClientRevision>,
    ) -> Result<StoredCatalogue, ControlPlaneError> {
        let read = self.catalogue_at(expected).await?;
        self.check_application_id_available(&command)?;
        let operation = command.operation();
        let updated = read
            .stored
            .catalogue
            .apply(command, operator.subject(), self.clock.now_unix_seconds())
            .map_err(ControlPlaneError::InvalidRequest)?;
        read.save(operator, operation, updated, expected).await
    }
}

impl CatalogueRead {
    /// Writes `updated` over the catalogue at `expected`, to the repository
    /// it was read from, and audits it: every catalogue write, a command
    /// applied or a selection, saves this one way.
    /// # Errors
    /// A catalogue too large to store, a lost race, and repository failures.
    pub(crate) async fn save(
        &self,
        operator: &Operator,
        operation: &str,
        updated: Catalogue,
        expected: Option<&ClientRevision>,
    ) -> Result<StoredCatalogue, ControlPlaneError> {
        // Measured before the write, not left to the repository to discover:
        // GitHub's contents API cannot read a file this size back, so a
        // catalogue that grows past it must be refused here rather than
        // committed and then unreadable.
        crate::document_size::check(&updated.render().map_err(ControlPlaneError::InvalidRequest)?)?;
        let change = ChangeContext {
            requested_by: operator.subject().into(),
            summary: "update product catalogue".into(),
        };
        // The read before this already refuses a revision this request never
        // saw, but a second write can still land between that read and this
        // one — the race this optimistic-concurrency write exists to catch.
        // `from_repository` would answer that as `RevisionConflict`, "the
        // client changed": correct for `RepositoryError::Conflict` from a
        // client write, but wrong here, where nothing about a client
        // changed. Mapped to `CatalogueRevisionConflict` instead, so a lost
        // race on the catalogue says what actually raced.
        let revision = self
            .repository
            .save_catalogue(&updated, expected, &change)
            .await
            .map_err(|error| match error {
                crate::RepositoryError::Conflict => ControlPlaneError::CatalogueRevisionConflict,
                other => ControlPlaneError::from_repository(other),
            })?;

        // The entry the catalogue decided this change touches is read back
        // off the record it just appended, rather than matched on the
        // command a second time — that is what keeps this event and the one
        // `GET /api/activity` shows unable to disagree. `operation`, unlike
        // that entry, is not taken from the activity record: the record's
        // `action` is the prose an operator reads there ("Application
        // created"), and the audit trail's own field follows the other
        // audit events' `snake_case` instead.
        let entry = updated
            .activity
            .last()
            .map_or("catalogue", |event| event.resource.as_str());
        audit::catalogue_changed(operator, operation, entry, &revision);

        Ok(StoredCatalogue {
            catalogue: updated,
            revision: Some(revision),
        })
    }
}
