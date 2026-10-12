//! What a client looks like on the wire.

use fabric_client_model::{ClientId, ClientRevision, Host, PrivateNetworkScheme, RealmName};

use crate::models::{LifecycleResponse, ProjectionResponse};
use crate::StoredClient;

/// One client, as the list and detail endpoints render it.
///
/// The realm is included in the overview because it is the operator's anchor
/// between this screen and everything else about the client — but the roles
/// and application clients are not, because those belong to the identity view
/// and duplicating them here would create two places a stale copy could
/// appear.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientResponse {
    /// Which client this is.
    pub(crate) id: ClientId,

    /// The name an operator sees.
    pub(crate) display_name: String,

    /// The hostnames its applications are reached on.
    pub(crate) hosts: Vec<Host>,

    /// The scheme its `.internal` callbacks use, when it declares one
    /// (D01-15). `null` otherwise, including on a document written before
    /// the field existed.
    pub(crate) private_network_scheme: Option<PrivateNetworkScheme>,

    /// The realm its identity lives in.
    pub(crate) realm: RealmName,

    /// The desired-state revision this was read at.
    pub(crate) revision: ClientRevision,

    /// Whether this is saved intent, a usable client, or retired.
    pub(crate) lifecycle: LifecycleResponse,

    /// Whether its stored projected identity clients are still what its own
    /// product configuration projects (D01-19b). On the overview and the
    /// list, so a divergence is seen before anyone opens the client.
    pub(crate) projection: ProjectionResponse,
}

impl ClientResponse {
    /// Renders a stored client, with the lifecycle derived for it.
    pub(crate) fn from_stored(stored: &StoredClient, lifecycle: LifecycleResponse) -> Self {
        let client = stored.document.client();

        Self {
            id: client.id.clone(),
            display_name: client.display_name.clone(),
            hosts: client.hosts.clone(),
            private_network_scheme: client.private_network_scheme,
            realm: client.identity.realm.clone(),
            revision: stored.revision.clone(),
            lifecycle,
            projection: stored.document.projection_check().into(),
        }
    }
}

/// The list endpoint's body.
///
/// An object with one field rather than a bare array, so the response can grow
/// a paging cursor without becoming a different shape.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ClientListResponse {
    /// Every client the repository holds.
    pub(crate) clients: Vec<ClientResponse>,
}
