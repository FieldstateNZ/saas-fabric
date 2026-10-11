//! The per-resource guards: a tenant binding or DataSource must advance past
//! the revision it was last published at to change (O1, O3), and a held
//! tenant may not silently leave the tenants document (O4).

use std::collections::HashMap;

use fabric_core::BindingRevision;
use serde::Serialize;

use crate::{
    DocumentKind, Emptying, PublicationError, PublishedResources, RuntimeSnapshot, TenantBindingDocument,
};

/// Refuses a publication that offers any resource without advancing it, or
/// that drops a held tenant.
///
/// The reader applies each resource under its own revision: it ignores an
/// older one and refuses a same-revision change, keeping its held copy in
/// both cases while applying everything else in the document. Publishing
/// such a resource would therefore hand readers a generation they can only
/// part-apply. A resource the held document holds is compared with that
/// copy. One it does not hold is compared with the held record's history: a DataSource
/// retired and declared again, or a tenant reverted out and placed again,
/// must come back above the highest revision it was ever published at, since
/// a reader that never applied its removal still holds that revision.
///
/// A held tenant missing from the offered tenants document is refused unless
/// the whole document is being emptied with [`Emptying::Intended`]: D01-27
/// allows no blind removal, and a Git revert of `placements.yaml` is the
/// ordinary way one would otherwise happen.
///
/// # Errors
///
/// [`PublicationError::ResourceRevisionNotAdvanced`] for the first resource
/// that does not advance, data sources first, then
/// [`PublicationError::TenantRemoved`] for the first tenant dropped.
pub(crate) fn refuse_resource_changes(
    snapshot: &RuntimeSnapshot,
    held: &PublishedResources,
) -> Result<(), PublicationError> {
    let history = &held.history;
    refuse_unadvanced(
        DocumentKind::DataSources,
        &held.data_sources,
        &snapshot.data_sources.payload,
        |source| {
            (
                source.id.to_string(),
                source.revision,
                history.data_source(&source.id),
            )
        },
    )?;
    refuse_unadvanced(
        DocumentKind::Tenants,
        &held.tenants,
        &snapshot.tenants.payload,
        |tenant| {
            (
                tenant.tenant.to_string(),
                tenant.revision,
                history.tenant(&tenant.tenant),
            )
        },
    )?;
    refuse_removed_tenants(snapshot, &held.tenants)
}

fn refuse_unadvanced<T: Serialize>(
    document: DocumentKind,
    held: &[T],
    offered: &[T],
    identity: impl Fn(&T) -> (String, BindingRevision, Option<BindingRevision>),
) -> Result<(), PublicationError> {
    let held: HashMap<String, (BindingRevision, &T)> = held
        .iter()
        .map(|resource| {
            let (id, revision, _) = identity(resource);
            (id, (revision, resource))
        })
        .collect();

    for resource in offered {
        let (id, offered_revision, published) = identity(resource);
        let held_copy = held.get(&id);
        let floor = held_copy.map(|(revision, _)| *revision).max(published);
        let Some(floor) = floor else {
            continue;
        };
        let advanced = offered_revision > floor
            || held_copy.is_some_and(|(held_revision, held_resource)| {
                offered_revision == *held_revision
                    && *held_revision == floor
                    && same_content(resource, *held_resource)
            });
        if !advanced {
            return Err(PublicationError::ResourceRevisionNotAdvanced {
                document,
                resource: id,
                held: floor,
                offered: offered_revision,
            });
        }
    }
    Ok(())
}

fn refuse_removed_tenants(
    snapshot: &RuntimeSnapshot,
    held_tenants: &[TenantBindingDocument],
) -> Result<(), PublicationError> {
    let offered = &snapshot.tenants;
    if offered.payload.is_empty() && offered.emptying == Emptying::Intended {
        return Ok(());
    }
    for held in held_tenants {
        if !offered.payload.iter().any(|tenant| tenant.tenant == held.tenant) {
            return Err(PublicationError::TenantRemoved {
                tenant: held.tenant.clone(),
                held: held.revision,
            });
        }
    }
    Ok(())
}

fn same_content<T: Serialize>(offered: &T, held: &T) -> bool {
    match (serde_json::to_value(offered), serde_json::to_value(held)) {
        (Ok(offered), Ok(held)) => offered == held,
        _ => false,
    }
}
