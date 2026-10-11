//! Leaving withheld tenants out of what a pass composes.

#[cfg(test)]
#[path = "withhold_tests.rs"]
mod withhold_tests;

use std::collections::{BTreeMap, BTreeSet};

use fabric_core::{BindingRevision, TenantId};
use fabric_runtime_publication::TenantBindingDocument;

use crate::PlacementRecord;

/// The placements a pass may compose: every record except those of a
/// withheld tenant.
///
/// The withheld tenant's records stay in `placements.yaml`, untouched: a
/// tombstone retains state, and only publication is refused. Dropping the
/// tenant from the offered document is what the runtime reads as "no longer
/// served" (ADR 0023 amendment). The publisher still refuses taking the
/// whole tenants document to empty unless that is intended, so withholding
/// the last published tenant is refused rather than published.
pub(super) fn publishable(
    placements: Vec<PlacementRecord>,
    withheld: &BTreeSet<TenantId>,
) -> Vec<PlacementRecord> {
    placements
        .into_iter()
        .filter(|placement| !withheld.contains(&placement.tenant))
        .collect()
}

/// Each tenant an accepted tenants document carries, at its binding
/// revision.
pub(super) fn published_tenants(tenants: &[TenantBindingDocument]) -> BTreeMap<TenantId, BindingRevision> {
    tenants
        .iter()
        .map(|binding| (binding.tenant.clone(), binding.revision))
        .collect()
}
