//! What an operator may send when changing a client's identity.

use fabric_client_model::{IdentityConfiguration, OidcClient, RealmName, RoleName};

/// A requested identity configuration.
///
/// # It is a replacement, not a patch
///
/// The operator sends the identity they want the client to have, in full, and
/// what they send is what the document ends up saying. That is `PUT`
/// semantics, and it is the right shape here for a reason specific to this
/// domain: a patch that omitted `roles` would be ambiguous between "leave the
/// roles alone" and "the client should have no roles", and one of those
/// readings quietly removes a role every user of the client is granted.
///
/// Unknown fields are refused rather than ignored. An operator — or a UI —
/// sending a field this version does not understand is sending an intention
/// the platform cannot honour, and accepting it silently would report success
/// for a change that did not happen.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct IdentityRequest {
    /// The realm the client's identity lives in.
    ///
    /// Present, and required, even though it cannot be changed — the document
    /// the operator is replacing has one, so a request that omitted it would
    /// be describing a client with no realm. Attempting to *change* it is
    /// refused explicitly rather than ignored; see
    /// [`ControlPlaneError::RealmImmutable`](crate::ControlPlaneError::RealmImmutable).
    pub(crate) realm: RealmName,

    /// The realm roles the client should have.
    pub(crate) roles: Vec<RoleName>,

    /// The application clients the realm should hold.
    #[serde(default)]
    pub(crate) clients: Vec<OidcClient>,
}

impl From<IdentityRequest> for IdentityConfiguration {
    fn from(request: IdentityRequest) -> Self {
        Self {
            realm: request.realm,
            roles: request.roles,
            clients: request.clients,
        }
    }
}
