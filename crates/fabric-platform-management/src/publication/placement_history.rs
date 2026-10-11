//! Holding recorded placements to the tenants document last published from
//! them (O4, D01-27): placements roll forward only.

use fabric_core::BindingRevision;
use fabric_runtime_publication::TenantBindingDocument;

use super::compose_error::ComposeError;
use crate::placements::PlacementRecord;

/// Refuses placements that no longer account for a tenant as it was last
/// published: a logical binding with no record left, or records whose
/// revisions no longer reach the tenant's published revision.
///
/// A tenant's published revision is the sum of its records' revisions
/// ([`compose`](super::compose)). Summing only the records for the logical
/// data sources it was published with, a Git revert that lowers one record
/// shows here even when the same revert adds another record and the
/// tenant's total still rises; the publisher, which sees only that total,
/// would accept it. A record whose binding changed must raise that sum: the
/// explicit revision-bumping write D01-27 requires. One record lowered while
/// another of the same tenant is raised in the same write is not detected;
/// the raise is itself such a write.
///
/// # Errors
///
/// [`ComposeError::PlacementRemoved`] or [`ComposeError::PlacementLowered`]
/// for the first tenant, in published order, that fails.
pub(super) fn refuse_removed_or_lowered(
    placements: &[PlacementRecord],
    published: &[TenantBindingDocument],
) -> Result<(), ComposeError> {
    for tenant in published {
        let mut kept: u64 = 0;
        let mut changed = false;
        for (logical, binding) in &tenant.data {
            let Some(record) = placements
                .iter()
                .find(|record| record.tenant == tenant.tenant && record.logical == *logical)
            else {
                return Err(ComposeError::PlacementRemoved {
                    tenant: tenant.tenant.clone(),
                    logical: logical.clone(),
                });
            };
            kept = kept.saturating_add(record.revision.get());
            changed |= record.data_source != binding.data_source || record.isolation != binding.isolation;
        }
        let kept = BindingRevision::new(kept);
        if kept < tenant.revision || (changed && kept == tenant.revision) {
            return Err(ComposeError::PlacementLowered {
                tenant: tenant.tenant.clone(),
                published: tenant.revision,
                recorded: kept,
            });
        }
    }
    Ok(())
}
