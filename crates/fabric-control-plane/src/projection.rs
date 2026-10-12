//! Refusing to act on a stored projected client that is no longer what the
//! stored document projects (D01-19b).
//!
//! A product save would replace the diverged entry whole, silently undoing
//! it; a re-assertion would write it over a hand edit in the identity
//! provider as though it were the projection. Both are refused by this one
//! guard, so the two cannot disagree about what a divergence is. A sweep is
//! not refused: it withholds the entries `ProjectionCheck::withheld` names
//! and converges the rest (see `reconcile::pass`).

use fabric_client_model::{ClientDocument, ClientId};

use crate::ControlPlaneError;

/// Refuses when a stored projected entry differs from the stored document's
/// own projection, naming the first such client and its field, never a
/// callback.
///
/// A document whose projection cannot be computed is a stored document the
/// platform cannot read, as a product save has always reported it.
pub(crate) fn require(document: &ClientDocument, id: &ClientId) -> Result<(), ControlPlaneError> {
    let divergences =
        document
            .projection_divergences()
            .map_err(|source| ControlPlaneError::InvalidDesiredState {
                client: id.clone(),
                source,
            })?;

    match divergences.into_iter().next() {
        Some(divergence) => Err(ControlPlaneError::ProjectedClientDiverged {
            client: divergence.client,
            field: divergence.field.as_str(),
        }),
        None => Ok(()),
    }
}
