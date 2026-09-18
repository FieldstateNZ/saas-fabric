//! Reparsing a client id as the tenant id a placement is recorded under.

use fabric_core::TenantId;

use crate::placements::refusal::PlacementRefusal;

/// Reparses a client id as the [`TenantId`] a placement is recorded under.
///
/// # Why this can fail, and why it should not
///
/// `ClientId` and `TenantId` validate with the same rule, so this never
/// fails for an id the console created -- see
/// `docs/architecture/client-desired-state.md`. It is checked anyway
/// rather than trusted, because the record this produces is what the
/// runtime reads. Shared by [`Placements::place`](crate::Placements::place)
/// and [`Placements::for_client`](crate::Placements::for_client) (ADR 0023
/// part 2, N11), which is why it is its own file rather than a private
/// helper on either.
pub(super) fn tenant_id(client: &str) -> Result<TenantId, PlacementRefusal> {
    TenantId::try_new(client).map_err(|_error| PlacementRefusal::TenantIdInvalid {
        client: client.to_owned(),
    })
}
