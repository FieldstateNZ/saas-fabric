//! Rules 3-5: choosing among candidates `select` has already filtered.

use fabric_core::{LogicalDataSourceName, TenantId};
use fabric_runtime_publication::IsolationModelDocument;

use crate::data_sources::{DataSourceDeclaration, Discriminator};
use crate::placements::intent::DataIntent;
use crate::placements::record::PlacementRecord;
use crate::placements::refusal::PlacementRefusal;

/// Rule 3: the least-loaded shared candidate, isolated by discriminator.
pub(super) fn pick_shared(
    intent: &DataIntent,
    tenant: &TenantId,
    logical: &LogicalDataSourceName,
    candidates: Vec<&DataSourceDeclaration>,
    held: &[PlacementRecord],
    now: &str,
) -> Result<PlacementRecord, PlacementRefusal> {
    // `filter_map` pulls the declared column out alongside the candidate,
    // so a shared candidate with no discriminator (declare-time validation
    // guarantees there is none) is simply not ranked -- rather than ranked
    // and then re-checked, which is the unreachable branch this used to
    // carry.
    let mut ranked: Vec<(&DataSourceDeclaration, &Discriminator, usize)> = candidates
        .into_iter()
        .filter_map(|candidate| {
            let discriminator = candidate.discriminator.as_ref()?;
            let count = held
                .iter()
                .filter(|placed| placed.data_source == candidate.id)
                .count();
            Some((candidate, discriminator, count))
        })
        .collect();
    ranked.sort_by(|left, right| left.2.cmp(&right.2).then_with(|| left.0.id.cmp(&right.0.id)));

    let Some((chosen, discriminator, _)) = ranked.into_iter().next() else {
        return Err(no_data_source_admits(intent));
    };

    let value = tenant.as_str().to_owned();
    // A collision is two *different* tenants recorded at one value, never
    // this tenant's own second logical placement on the same source: the
    // value is always this tenant's id (ADR 0023 part 2), so a tenant with
    // `primary` and `audit` both shared on one source repeats its own
    // value on purpose, and that is one tenant with one key in one
    // database -- nothing collides.
    let taken = held.iter().any(|placed| {
        placed.data_source == chosen.id
            && &placed.tenant != tenant
            && matches!(&placed.isolation, IsolationModelDocument::Discriminator { value: existing, .. } if existing == &value)
    });
    if taken {
        return Err(PlacementRefusal::DiscriminatorValueTaken {
            data_source: chosen.id.clone(),
            value,
        });
    }

    Ok(PlacementRecord {
        tenant: tenant.clone(),
        logical: logical.clone(),
        data_source: chosen.id.clone(),
        isolation: IsolationModelDocument::Discriminator {
            column: discriminator.column.clone(),
            value,
        },
        placed_at: now.to_owned(),
    })
}

/// Rule 4: the lowest-id candidate with no held placement, isolated as a
/// whole database.
pub(super) fn pick_exclusive(
    intent: &DataIntent,
    tenant: &TenantId,
    logical: &LogicalDataSourceName,
    candidates: Vec<&DataSourceDeclaration>,
    held: &[PlacementRecord],
    now: &str,
) -> Result<PlacementRecord, PlacementRefusal> {
    // Distinguished so the refusal can tell "nothing declared admits this"
    // from "something did, and it is already taken" -- the operator's next
    // action differs, and only the first one is something to declare.
    let any_matched = !candidates.is_empty();

    let mut free: Vec<&DataSourceDeclaration> = candidates
        .into_iter()
        .filter(|candidate| !held.iter().any(|placed| placed.data_source == candidate.id))
        .collect();
    free.sort_by(|left, right| left.id.cmp(&right.id));

    let Some(chosen) = free.into_iter().next() else {
        return Err(if any_matched {
            PlacementRefusal::AllMatchingSourcesOccupied { class: intent.class }
        } else {
            no_data_source_admits(intent)
        });
    };

    Ok(PlacementRecord {
        tenant: tenant.clone(),
        logical: logical.clone(),
        data_source: chosen.id.clone(),
        isolation: IsolationModelDocument::Database {},
        placed_at: now.to_owned(),
    })
}

/// Rule 5's refusal, built from the intent nothing could admit.
pub(super) fn no_data_source_admits(intent: &DataIntent) -> PlacementRefusal {
    PlacementRefusal::NoDataSourceAdmits {
        class: intent.class,
        region: intent.region.clone(),
        provider: intent.provider.clone(),
    }
}
