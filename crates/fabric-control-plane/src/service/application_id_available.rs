//! Whether a new application id is this platform's own, checked before the
//! id is ever chosen.

use fabric_client_model::{catalogue::CatalogueCommand, DesiredStateError};

use crate::{ClientService, ControlPlaneError};

impl ClientService {
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
    pub(super) fn check_application_id_available(
        &self,
        command: &CatalogueCommand,
    ) -> Result<(), ControlPlaneError> {
        let CatalogueCommand::CreateApplication { id, .. } = command else {
            return Ok(());
        };

        // `reserved_client_ids` holds case-folded strings (see
        // `fabric_control_plane_api::startup::reserved_names::client_ids`
        // for why), and `id` is already lowercase by construction —
        // `ClientId::try_new` only accepts lowercase ASCII — so no folding
        // is needed on this side, the same as the realm check.
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
