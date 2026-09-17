//! Whether a realm is this operator's to take, checked before a client is
//! ever created under it.

use fabric_client_model::{ClientId, DesiredStateError, RealmName};

use crate::{ClientService, ControlPlaneError, RealmUnavailableReason};

impl ClientService {
    /// Refuses a realm that is reserved, or that another stored client
    /// already declares.
    ///
    /// # Why this matters
    ///
    /// [`ClientDocument::create`](fabric_client_model::ClientDocument::create)
    /// sets a new client's realm to its own id, and reconciliation treats
    /// *any* realm the document names as this client's — including one
    /// that already exists for another reason. Keycloak answers `409` to a
    /// realm-create call that finds the realm already there, and this
    /// platform's own admin client treats that as success (see
    /// `fabric_keycloak::admin`'s `create`), so the very next sweep would
    /// rename that realm, add roles to it and write application clients
    /// into it — using this operator's own bearer. A client id of
    /// `master`, or one matching a realm another client document already
    /// declares by hand, is exactly the takeover this refuses before a
    /// document is ever written.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError::RealmUnavailable`] if the realm this id
    /// would produce is reserved, or is already declared by another client's
    /// stored document.
    pub(super) async fn check_realm_available(&self, id: &ClientId) -> Result<(), ControlPlaneError> {
        let realm = RealmName::try_new(id.as_str()).map_err(|error| {
            ControlPlaneError::InvalidRequest(DesiredStateError::InvalidField {
                field: "id",
                detail: error.to_string(),
            })
        })?;

        // `reserved_realms` holds plain, case-folded strings rather than
        // `RealmName` (see `ClientService`'s own field for why), but `realm`
        // is already lowercase by construction — `RealmName::try_new` only
        // accepts lowercase ASCII — so no folding is needed on this side.
        if self.reserved_realms().contains(realm.as_str()) {
            return Err(ControlPlaneError::RealmUnavailable {
                realm,
                reason: RealmUnavailableReason::Reserved,
            });
        }

        let clients = self
            .repository
            .current()
            .list()
            .await
            .map_err(ControlPlaneError::from_repository)?;

        // Excludes a client already stored under *this same* id: realm and
        // id are the same string by construction, so re-requesting an id
        // that already exists would otherwise always collide with itself
        // here first — reported as a realm conflict with some other tenant,
        // when it is a plain duplicate id. That one is `create`'s own job,
        // in `create_client.rs`, which answers it as `ClientExists`.
        if clients.iter().any(|stored| {
            stored.document.client().identity.realm == realm && stored.document.client().id != *id
        }) {
            return Err(ControlPlaneError::RealmUnavailable {
                realm,
                reason: RealmUnavailableReason::Taken,
            });
        }

        Ok(())
    }
}
