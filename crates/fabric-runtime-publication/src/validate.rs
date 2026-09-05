//! The whole-snapshot refusal rules, run before any document's verdict is
//! even computed.
//!
//! Every function here is pure: no filesystem, no adapter — just the
//! incoming snapshot and, where a rule needs it, the *held* tenants document
//! an adapter has already read and parsed. That is what makes the emptying
//! guard and the empty-catalogue refusal unit-testable the same way the
//! crate's internal verdict computation is.
//!
//! This file is in the 121-150 line band the file-size policy asks a reason
//! for: it is six small, independently-testable guard functions plus the one
//! function that runs them in order, and every guard's rustdoc names the ADR
//! 0018 rule it enforces. Splitting the guards into their own files would
//! scatter `validate_snapshot`'s reading order across the module tree for no
//! reader's benefit -- the whole point of this file is that the order is
//! visible in one place.

use crate::{
    CatalogDocument, DataSourceDocument, DocumentKind, Emptying, PublicationError, RuntimeSnapshot,
    TenantBindingDocument,
};

/// Runs every whole-snapshot rule, in the order a reader would want to know
/// about a problem: the two unusable-shape checks first, then the two
/// referential checks, then the two emptying guards.
///
/// `held_tenants` and `held_data_sources` are the *held* documents — what an
/// adapter read off disk before this publication was offered — never the
/// documents inside `snapshot` itself.
///
/// # Errors
///
/// The first rule this snapshot violates, as a [`PublicationError`].
pub(crate) fn validate_snapshot(
    snapshot: &RuntimeSnapshot,
    held_tenants: &[TenantBindingDocument],
    held_data_sources: &[DataSourceDocument],
) -> Result<(), PublicationError> {
    refuse_empty_catalogue(&snapshot.catalog.payload)?;
    refuse_empty_tenant_data(&snapshot.tenants.payload)?;
    refuse_dangling_data_sources(&snapshot.tenants.payload, &snapshot.data_sources.payload)?;
    refuse_retired_data_source_still_bound(held_tenants, &snapshot.data_sources.payload)?;
    refuse_unintended_emptying(
        DocumentKind::Tenants,
        held_tenants.len(),
        snapshot.tenants.payload.len(),
        snapshot.tenants.emptying,
    )?;
    refuse_unintended_emptying(
        DocumentKind::DataSources,
        held_data_sources.len(),
        snapshot.data_sources.payload.len(),
        snapshot.data_sources.emptying,
    )
}

/// ADR 0018 part 2 / decision 6: an empty catalogue has no bootstrap value,
/// so it is refused unconditionally — whatever the `Emptying` intent says.
fn refuse_empty_catalogue(catalog: &CatalogDocument) -> Result<(), PublicationError> {
    if catalog.is_empty() {
        return Err(PublicationError::EmptyCatalogue);
    }
    Ok(())
}

/// Symmetric with `refuse_empty_catalogue`: a tenant whose `data` map is
/// empty is reachable only through `Deserialize`
/// (`TenantDataBindings::try_new` refuses one at construction), but nothing
/// here would otherwise stop it from being published. The consumer's own
/// `TenantRuntimeBinding::validate` makes the same refusal, but only after
/// `ResourceRegistry::apply_all` has already dropped the whole binding and
/// kept whatever was held before — a publication that looked like it changed
/// something, silently discarded on arrival. Refused here instead.
fn refuse_empty_tenant_data(tenants: &[TenantBindingDocument]) -> Result<(), PublicationError> {
    for tenant in tenants {
        if tenant.data.is_empty() {
            return Err(PublicationError::EmptyTenantData {
                tenant: tenant.tenant.clone(),
            });
        }
    }
    Ok(())
}

/// ADR 0018 part 4: a tenants document naming a `DataSourceId` this same
/// publication's data-sources document lacks is refused before any write.
fn refuse_dangling_data_sources(
    tenants: &[TenantBindingDocument],
    data_sources: &[DataSourceDocument],
) -> Result<(), PublicationError> {
    for tenant in tenants {
        for (logical, binding) in &tenant.data {
            if !data_sources
                .iter()
                .any(|data_source| data_source.id == binding.data_source)
            {
                return Err(PublicationError::DanglingDataSource {
                    tenant: tenant.tenant.clone(),
                    logical: logical.clone(),
                    data_source: binding.data_source.clone(),
                });
            }
        }
    }
    Ok(())
}

/// ADR 0018 part 3 / decision 12: a data-sources document may not drop an id
/// the *held* tenants document still references. Checked against the held
/// document, never the incoming one, so retiring a DataSource is genuinely
/// two publications — one that unbinds it, then a second that drops it once
/// the first is held.
fn refuse_retired_data_source_still_bound(
    held_tenants: &[TenantBindingDocument],
    data_sources: &[DataSourceDocument],
) -> Result<(), PublicationError> {
    for tenant in held_tenants {
        for binding in tenant.data.values() {
            if !data_sources
                .iter()
                .any(|data_source| data_source.id == binding.data_source)
            {
                return Err(PublicationError::RetiredDataSourceStillBound {
                    data_source: binding.data_source.clone(),
                    tenant: tenant.tenant.clone(),
                });
            }
        }
    }
    Ok(())
}

/// ADR 0018 part 6: refuses taking a document from non-empty to empty unless
/// the caller stated that intent. An empty document staying empty is not an
/// emptying at all, so `held_len == 0` never refuses regardless of intent.
fn refuse_unintended_emptying(
    document: DocumentKind,
    held_len: usize,
    incoming_len: usize,
    emptying: Emptying,
) -> Result<(), PublicationError> {
    let is_emptying = held_len > 0 && incoming_len == 0;

    if is_emptying && emptying == Emptying::NotIntended {
        return Err(PublicationError::EmptyingNotIntended { document });
    }
    Ok(())
}
