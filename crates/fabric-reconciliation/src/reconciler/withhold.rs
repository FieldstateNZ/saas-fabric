//! Leaving declared application clients out of a pass (D01-19b).

use fabric_client_model::{Client, OidcClientId};

/// `client` without the declared clients `withheld` names, and the ids of
/// those it actually left out. An id `client` does not declare is not
/// withheld: there was nothing to converge.
///
/// Leaving a client out of the declaration is enough to leave it alone:
/// reconciliation plans only for declared clients, and never deletes one.
pub(super) fn split(client: &Client, withheld: &[OidcClientId]) -> (Client, Vec<OidcClientId>) {
    let mut declared = client.clone();
    let (kept, left_out): (Vec<_>, Vec<_>) = declared
        .identity
        .clients
        .into_iter()
        .partition(|declared| !withheld.contains(&declared.id));
    declared.identity.clients = kept;

    (declared, left_out.into_iter().map(|client| client.id).collect())
}
