//! What a client's identity looks like on the wire.

use fabric_client_model::{ClientRevision, OidcClient, RealmName, RoleName};

use crate::models::ReconciliationResponse;
use crate::repository::StoredClient;

/// A client's identity, plus what is known about whether it has taken effect.
///
/// # The two halves are the point
///
/// `realm`, `roles` and `clients` are **desired state** — what Git says.
/// `reconciliation` is what is known about the **actual** state of the
/// identity provider. Returning them together, from one endpoint, is what
/// stops an operator reading a document and assuming it is reality; returning
/// them separately would make the second call the one nobody makes.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdentityResponse {
    /// The realm the client's identity lives in.
    pub(crate) realm: RealmName,

    /// The realm roles the client should have.
    pub(crate) roles: Vec<RoleName>,

    /// The application clients the realm should hold.
    pub(crate) clients: Vec<OidcClient>,

    /// The document's schema version — `fabric.fieldstate.nz/v1` or `.../v2`.
    ///
    /// ADR 0019 requires this to reach the console: an operator "should not
    /// discover the version change in a Git diff", and the version badge is
    /// how anyone knows which is which. Read from
    /// [`ClientDocument::api_version`](fabric_client_model::ClientDocument::api_version)
    /// rather than assumed, so a document nobody has edited keeps reporting
    /// the version it actually declares.
    pub(crate) api_version: &'static str,

    /// The desired-state revision this was read at.
    ///
    /// Also sent as the response's entity tag. It is in the body as well
    /// because a browser is not always able to read `ETag` across an origin,
    /// and a UI that could not obtain the revision could not make a safe write
    /// at all.
    pub(crate) revision: ClientRevision,

    /// Where reconciliation stands for this revision.
    pub(crate) reconciliation: ReconciliationResponse,
}

impl IdentityResponse {
    /// Renders a stored client's identity and its reconciliation state.
    ///
    /// Takes the whole [`StoredClient`] rather than its pieces so that
    /// `api_version` is read from the same document the identity and
    /// revision come from — there is no way to pass a version that belongs
    /// to a different document than the one being rendered.
    pub(crate) fn new(stored: &StoredClient, reconciliation: ReconciliationResponse) -> Self {
        let identity = &stored.document.client().identity;
        Self {
            realm: identity.realm.clone(),
            roles: identity.roles.clone(),
            clients: identity.clients.clone(),
            api_version: stored.document.api_version(),
            revision: stored.revision.clone(),
            reconciliation,
        }
    }
}
