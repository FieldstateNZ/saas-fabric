//! Which isolation kind a placement class may serve.

use fabric_runtime_publication::{IsolationModelDocument, PlacementClassDocument};

/// Whether this isolation kind is the one its data source's placement
/// class may serve. A shared source serves discriminator isolation only
/// (ADR 0006); every other class serves a whole database. `schema`
/// isolation matches nothing -- nothing in this platform produces it yet.
pub(super) fn isolation_matches_class(
    placement: PlacementClassDocument,
    isolation: &IsolationModelDocument,
) -> bool {
    matches!(
        (placement, isolation),
        (
            PlacementClassDocument::Shared,
            IsolationModelDocument::Discriminator { .. }
        ) | (
            PlacementClassDocument::Dedicated
                | PlacementClassDocument::HighAvailability
                | PlacementClassDocument::Regulated
                | PlacementClassDocument::Development
                | PlacementClassDocument::Ephemeral,
            IsolationModelDocument::Database {},
        )
    )
}
