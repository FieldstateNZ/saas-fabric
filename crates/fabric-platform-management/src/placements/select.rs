//! Choosing where a tenant's data intent lands. The only place that decides.

mod pick;
#[cfg(test)]
#[path = "select_tests.rs"]
mod select_tests;

use fabric_core::{LogicalDataSourceName, TenantId};
use fabric_runtime_publication::PlacementClassDocument;

use crate::data_sources::DataSourceDeclaration;
use crate::placements::intent::DataIntent;
use crate::placements::record::PlacementRecord;
use crate::placements::refusal::PlacementRefusal;
use crate::placements::select::pick::{pick_exclusive, pick_shared};

/// Chooses which declared data source a tenant's data intent should land
/// on, from what is declared and what is already held.
///
/// Pure: everything it needs is a parameter, including `now`, so a test
/// needs no clock, no repository and no I/O. This is the only place ADR
/// 0023 part 2 lets that decision be made -- `Placements::place` writes
/// exactly what this returns, and `Placements::for_client` previews it
/// without writing.
///
/// # Errors
///
/// A [`PlacementRefusal`] naming which of the rules below stopped it:
///
/// 1. [`AlreadyPlaced`](PlacementRefusal::AlreadyPlaced) if `held` already
///    has this (tenant, logical) pair.
/// 2. Candidates are declared data sources whose `placement` equals
///    `intent.class`, whose `capabilities.accepts_new_tenants` and
///    `capabilities.writable` are both true, and -- when `intent.region` is
///    stated -- whose `residency.region` matches it exactly. `provider` is
///    never matched; nothing declares one.
/// 3. On a `shared` class, the candidate with the fewest held placements
///    wins (ties broken by the lowest id), isolated by a discriminator
///    naming the candidate's own declared column and this tenant's id as
///    the value -- refusing
///    [`DiscriminatorValueTaken`](PlacementRefusal::DiscriminatorValueTaken)
///    if a held placement on it already carries that value. See
///    `select/pick.rs`.
/// 4. On any other class, a data source is one tenant's: candidates with no
///    held placement at all are considered, and the lowest id wins.
///    Isolation is always `database {}`; `schema` isolation is never
///    produced (ADR 0006 calls it inert). See `select/pick.rs`.
/// 5. No candidate at either step 3 or 4 is
///    [`NoDataSourceAdmits`](PlacementRefusal::NoDataSourceAdmits).
pub fn select(
    intent: &DataIntent,
    tenant: &TenantId,
    logical: &LogicalDataSourceName,
    declared: &[DataSourceDeclaration],
    held: &[PlacementRecord],
    now: &str,
) -> Result<PlacementRecord, PlacementRefusal> {
    if held
        .iter()
        .any(|placed| &placed.tenant == tenant && &placed.logical == logical)
    {
        return Err(PlacementRefusal::AlreadyPlaced {
            tenant: tenant.clone(),
            logical: logical.clone(),
        });
    }

    let candidates: Vec<&DataSourceDeclaration> = declared
        .iter()
        .filter(|declared| declared.placement == intent.class)
        .filter(|declared| declared.capabilities.accepts_new_tenants && declared.capabilities.writable)
        .filter(|declared| {
            intent
                .region
                .as_deref()
                .is_none_or(|region| declared.residency.region == region)
        })
        .collect();

    if intent.class == PlacementClassDocument::Shared {
        pick_shared(intent, tenant, logical, candidates, held, now)
    } else {
        pick_exclusive(intent, tenant, logical, candidates, held, now)
    }
}
