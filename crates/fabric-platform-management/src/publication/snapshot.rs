//! Composing a complete `RuntimeSnapshot` from what the platform has
//! declared and recorded -- pure, so every rule here is a unit test rather
//! than something only a running pass exercises.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md):
//! [`compose`] and the one private helper that groups placements into
//! tenant bindings are a single cohesive computation over one input list,
//! and splitting the grouping step from the function that calls it would
//! separate the revision-sum rule (D1) from the only place it is applied.

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod snapshot_tests;

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    CatalogDocument, DataSourceDocument, DocumentInput, DocumentRevision, PublishedRevisions,
    RuntimeSnapshot, TenantBindingDocument, TenantDataBindingDocument, TenantDataBindings,
};

use crate::data_sources::DataSourceDeclaration;
use crate::placements::PlacementRecord;

/// Why [`compose`] could not build a snapshot from what it was given.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComposeError {
    /// One tenant's logical data source is recorded twice.
    ///
    /// Held state this crate itself wrote never has this shape --
    /// `select` refuses a second placement for one (tenant, logical) -- but
    /// a break-glass edit to `placements.yaml` can produce it, and a
    /// formula that silently kept one entry over the other would make a
    /// publication disagree with whichever entry an operator reads in the
    /// file. `check_held_placements` refuses this same shape for the
    /// console; this is the same rule at the seam a scheduled pass runs
    /// through even when nothing reads the list first.
    #[error("{tenant} has two placements for {logical}")]
    DuplicatePlacement {
        /// The tenant with two records.
        tenant: TenantId,
        /// The logical data source both records name.
        logical: LogicalDataSourceName,
    },
}

/// Builds the complete snapshot one publication pass offers, from the
/// platform's declared data sources, recorded placements, and the derived
/// catalogue.
///
/// Every document is offered at the revision `held` currently reports, or
/// `DocumentRevision::new(1)` when `held` has none. `compose` never
/// advances a revision itself -- that is the offer-and-advance protocol's
/// job, only once the target says the offered bytes diverge from what it
/// holds -- and never calls [`DocumentInput::emptying_intended`]: an
/// environment that has genuinely lost every data source or placement is
/// exactly what a scheduled pass cannot tell apart from a read that came
/// back empty by accident, and ADR 0018 part 6 exists for that case, not
/// this function.
///
/// # Errors
///
/// [`ComposeError::DuplicatePlacement`] if `placements` records one
/// tenant's logical data source twice.
pub fn compose(
    declarations: Vec<DataSourceDeclaration>,
    placements: &[PlacementRecord],
    catalog: CatalogDocument,
    held: &PublishedRevisions,
) -> Result<RuntimeSnapshot, ComposeError> {
    let mut data_sources: Vec<DataSourceDocument> = declarations
        .into_iter()
        .map(DataSourceDeclaration::into_document)
        .collect();
    data_sources.sort_by(|left, right| left.id.cmp(&right.id));

    let tenants = compose_tenants(placements)?;

    Ok(RuntimeSnapshot {
        tenants: DocumentInput::new(revision_or_first(held.tenants), tenants),
        data_sources: DocumentInput::new(revision_or_first(held.data_sources), data_sources),
        catalog: DocumentInput::new(revision_or_first(held.catalog), catalog),
    })
}

/// What one tenant's placements have accumulated into, while
/// [`compose_tenants`] walks the whole list once.
#[derive(Default)]
struct Accumulator {
    data: BTreeMap<LogicalDataSourceName, TenantDataBindingDocument>,
    revision: u64,
}

/// One tenant binding per tenant with at least one recorded placement, its
/// revision the sum of its records' own (ADR 0023 part 4, D1): summing --
/// rather than taking a maximum -- is what makes an add or a break-glass
/// bump to any one record always move the tenant's published revision
/// forward, without this function knowing which record changed.
///
/// `features`, `secrets`, `configuration` and `storage` have no producer in
/// this slice, so every binding publishes them empty on purpose rather than
/// inventing a value ADR 0018 would refuse to publish.
fn compose_tenants(placements: &[PlacementRecord]) -> Result<Vec<TenantBindingDocument>, ComposeError> {
    let mut by_tenant: BTreeMap<TenantId, Accumulator> = BTreeMap::new();

    for placement in placements {
        let accumulator = by_tenant.entry(placement.tenant.clone()).or_default();

        let binding = TenantDataBindingDocument {
            data_source: placement.data_source.clone(),
            isolation: placement.isolation.clone(),
        };
        if accumulator
            .data
            .insert(placement.logical.clone(), binding)
            .is_some()
        {
            return Err(ComposeError::DuplicatePlacement {
                tenant: placement.tenant.clone(),
                logical: placement.logical.clone(),
            });
        }

        accumulator.revision = accumulator.revision.saturating_add(placement.revision.get());
    }

    Ok(by_tenant
        .into_iter()
        .map(|(tenant, accumulator)| TenantBindingDocument {
            tenant,
            revision: BindingRevision::new(accumulator.revision),
            // A tenant only enters `by_tenant` alongside its first logical
            // binding, so `accumulator.data` is never empty here.
            data: TenantDataBindings::try_new(accumulator.data)
                .unwrap_or_else(|_| unreachable!("a tenant is only inserted alongside its first binding")),
            configuration: None,
            secrets: None,
            features: BTreeMap::new(),
            storage: BTreeMap::new(),
        })
        .collect())
}

/// The revision a document is offered at when nothing is held yet.
fn revision_or_first(held: Option<DocumentRevision>) -> DocumentRevision {
    held.unwrap_or(DocumentRevision::new(1))
}
