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
        // The read above already refuses a revision this request never saw,
        // but a second write can still land between that read and this one
        // — the race this optimistic-concurrency write exists to catch.
        // `from_repository` would answer that as `RevisionConflict`, "the
        // client changed": correct for `RepositoryError::Conflict` from a
        // client write, but wrong here, where nothing about a client
        // changed. Mapped to `CatalogueRevisionConflict` instead, so a
        // lost race on the catalogue says what actually raced.
        let revision = repository
            .save_catalogue(&updated, expected, &change)
            .await
            .map_err(|error| match error {
                crate::RepositoryError::Conflict => ControlPlaneError::CatalogueRevisionConflict,
                other => ControlPlaneError::from_repository(other),
            })?;

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
    ///
    /// # What this actually prevents
    ///
    /// Not a *new* client landing in the console's or the admin identity's
    /// own realm — `check_realm_available` already refuses that for every
    /// client created from here on. What it cannot reach is a client
    /// document that predates `check_realm_available` itself: realm
    /// reservation is a create-time check, so a client stored before it
    /// existed, whose realm already happens to be the operator realm or
    /// `admin_realm`, is grandfathered past it — its realm is immutable, so
    /// there is no later moment `check_realm_available` gets a second
    /// chance to refuse it. Assign such a client an application carrying
    /// the console's or the admin identity's own id, and `create_oidc_client`
    /// runs inside the one realm that already holds the platform's real
    /// client: Keycloak refuses the duplicate `clientId` with `409`, this
    /// platform's own admin client treats that as an idempotent create's
    /// success (see `fabric_keycloak::admin::requests::create`), and the
    /// next sweep finds the declared application "drifted" from what it
    /// observed and calls `update_oidc_client` — which looks the id up by
    /// its Keycloak-internal identifier and overwrites whatever it finds,
    /// the platform's own client. Refused here, at the one place the id is
    /// chosen, rather than left to surface as a corrupted platform client
    /// later.
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
