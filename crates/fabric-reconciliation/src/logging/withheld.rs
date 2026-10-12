//! The log event for declared clients a pass withheld (D01-19b).

use fabric_client_model::{Client, OidcClientId};
use fabric_core::{event_id, EventType};

use crate::DOMAIN_ID;

/// The caller withheld these declared application clients from the pass:
/// they were neither written nor corrected.
///
/// Warn: nothing failed, but the provider is not being converged for them
/// and nothing will change that until their declaration is trusted again.
pub(crate) fn clients_withheld(client: &Client, withheld: &[OidcClientId]) {
    let withheld = withheld
        .iter()
        .map(OidcClientId::as_str)
        .collect::<Vec<_>>()
        .join(",");
    tracing::warn!(
        event = "reconciliation.clients_withheld",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 2),
        client_id = %client.id,
        realm = %client.identity.realm,
        withheld = %withheld,
        "declared application clients withheld from convergence"
    );
}
