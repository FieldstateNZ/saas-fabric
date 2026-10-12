//! Refusing an identity edit to a product-owned projected client (D01-19b).

use fabric_client_model::{ClientDocument, IdentityConfiguration, OidcClient};

use crate::ControlPlaneError;

/// Refuses `requested` if it would change, add or remove an application
/// client projected from one of `current`'s assignments.
///
/// A projected client is the product's (ADR 0021 §4, D01-19b): a product
/// save replaces it whole, so an identity edit to it would last only until
/// the next save and the realm would be converged back — the silent fight
/// the disposition refuses. Entries an operator declared by hand are not
/// projected and are not checked here.
///
/// Not called for a retired client: product saves are refused there, so an
/// identity edit is the only way left to remediate a compromised callback.
///
/// # Errors
///
/// [`ControlPlaneError::ProjectedClientProductOwned`] naming the first
/// projected client the edit would change, and
/// [`ControlPlaneError::InvalidDesiredState`] if the stored product section
/// does not parse, so which clients are projected cannot be known.
pub(super) fn refuse_projected_change(
    current: &ClientDocument,
    requested: &IdentityConfiguration,
) -> Result<(), ControlPlaneError> {
    let stored = current.client();
    let product = current
        .product()
        .map_err(|source| ControlPlaneError::InvalidDesiredState {
            client: stored.id.clone(),
            source,
        })?;

    for assignment in &product.applications {
        let id = assignment.application_id.as_str();
        let before = find(&stored.identity.clients, id);
        let after = find(&requested.clients, id);

        if before != after {
            if let Some(client) = before.or(after) {
                return Err(ControlPlaneError::ProjectedClientProductOwned {
                    client: client.id.clone(),
                });
            }
        }
    }

    Ok(())
}

/// The application client with this id, if the list holds one.
fn find<'a>(clients: &'a [OidcClient], id: &str) -> Option<&'a OidcClient> {
    clients.iter().find(|client| client.id.as_str() == id)
}
