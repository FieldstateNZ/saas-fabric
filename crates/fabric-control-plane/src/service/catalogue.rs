//! Product catalogue operations over the current desired-state binding.
use crate::{audit, ChangeContext, ClientService, ControlPlaneError, Operator};
use fabric_client_model::{
    catalogue::{CatalogueCommand, StoredCatalogue},
    ClientRevision,
};
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
    /// Applies one catalogue command with optimistic concurrency.
    /// # Errors
    /// Rejects stale edits and invalid or inconsistent application definitions.
    pub async fn change_catalogue(
        &self,
        operator: &Operator,
        command: CatalogueCommand,
        expected: Option<&ClientRevision>,
    ) -> Result<StoredCatalogue, ControlPlaneError> {
        let repository = self.repository.current();
        let current = repository
            .catalogue()
            .await
            .map_err(ControlPlaneError::from_repository)?;
        if current.revision.as_ref() != expected {
            return Err(ControlPlaneError::RevisionConflict);
        }
        let updated = current
            .catalogue
            .apply(command, operator.subject(), self.clock.now_unix_seconds())
            .map_err(ControlPlaneError::InvalidRequest)?;
        let change = ChangeContext {
            requested_by: operator.subject().into(),
            summary: "update product catalogue".into(),
        };
        let revision = repository
            .save_catalogue(&updated, expected, &change)
            .await
            .map_err(ControlPlaneError::from_repository)?;

        // `apply` already decided the action and the entry it applies to —
        // reading them back off the record it just appended, rather than
        // matching on `command` a second time, is what keeps this event and
        // the one `GET /api/activity` shows unable to disagree.
        let (action, entry) = updated
            .activity
            .last()
            .map_or(("catalogue command applied", "catalogue"), |event| {
                (event.action.as_str(), event.resource.as_str())
            });
        audit::catalogue_changed(operator, action, entry, &revision);

        Ok(StoredCatalogue {
            catalogue: updated,
            revision: Some(revision),
        })
    }
}
