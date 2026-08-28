//! What a client looks like on the wire.

use fabric_client_model::{ClientId, ClientRevision, Host, RealmName};

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

    /// The realm its identity lives in.
    pub(crate) realm: RealmName,

    /// The desired-state revision this was read at.
    pub(crate) revision: ClientRevision,
}

impl ClientResponse {
    /// Renders a stored client.
    pub(crate) fn from_stored(stored: &StoredClient) -> Self {
        let client = stored.document.client();

        Self {
            id: client.id.clone(),
            display_name: client.display_name.clone(),
            hosts: client.hosts.clone(),
            realm: client.identity.realm.clone(),
            revision: stored.revision.clone(),
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
