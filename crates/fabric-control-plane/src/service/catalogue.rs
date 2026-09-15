//! Product catalogue operations over the current desired-state binding.
use crate::{audit, ChangeContext, ClientService, ControlPlaneError, Operator};
use fabric_client_model::{
    catalogue::{CatalogueCommand, StoredCatalogue},
    ClientRevision, DesiredStateError,
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
            return Err(ControlPlaneError::CatalogueRevisionConflict);
        }
        self.check_application_id_available(&command)?;
        let operation = command.operation();
        let updated = current
            .catalogue
            .apply(command, operator.subject(), self.clock.now_unix_seconds())
            .map_err(ControlPlaneError::InvalidRequest)?;
        // Measured before the write, not left to the repository to discover:
        // GitHub's contents API cannot read a file this size back, so a
        // catalogue that grows past it must be refused here rather than
        // committed and then unreadable.
        crate::document_size::check(&updated.render().map_err(ControlPlaneError::InvalidRequest)?)?;
        let change = ChangeContext {
            requested_by: operator.subject().into(),
            summary: "update product catalogue".into(),
        };
        let revision = repository
            .save_catalogue(&updated, expected, &change)
            .await
            .map_err(ControlPlaneError::from_repository)?;

        // The entry `apply` decided this command touches is read back off
        // the record it just appended, rather than matched on `command` a
        // second time — that is what keeps this event and the one
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

    /// Refuses a new application id that is this platform's own.
    ///
    /// [`Catalogue::apply`](fabric_client_model::catalogue::Catalogue::apply)
    /// already refuses the realm-managed built-ins every Keycloak realm
    /// carries (`account`, `realm-management`, …) — a static identity-
    /// protocol fact `fabric-client-model` knows on its own. What it cannot
    /// know is *this deployment's* own OIDC client ids: the console's, and,
    /// when Keycloak is configured, the platform's machine identity's.
    /// Publishing an application under either would make an assignment turn
    /// that client into a second application in its own realm the moment a
    /// client was assigned it.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError::InvalidRequest`] if `command` is
    /// [`CatalogueCommand::CreateApplication`] naming one of this platform's
    /// own ids.
    fn check_application_id_available(&self, command: &CatalogueCommand) -> Result<(), ControlPlaneError> {
        let CatalogueCommand::CreateApplication { id, .. } = command else {
            return Ok(());
        };

        if self.reserved_client_ids().contains(id.as_str()) {
            return Err(ControlPlaneError::InvalidRequest(
                DesiredStateError::InvalidField {
                    field: "id",
                    detail: "This id is reserved for the platform's own use".into(),
                },
            ));
        }

        Ok(())
    }
}
