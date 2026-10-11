//! The per-resource revision guard: a resource the held document already
//! holds must advance its revision to change.

use std::collections::HashMap;

use fabric_core::BindingRevision;
use serde::Serialize;

use crate::{DataSourceDocument, DocumentKind, PublicationError, RuntimeSnapshot, TenantBindingDocument};

/// Refuses a publication that offers any held tenant binding or DataSource
/// at an older revision, or at the same revision with different content.
///
/// The reader applies each resource under its own revision: it ignores an
/// older one and refuses a same-revision change, keeping its held copy in
/// both cases while applying everything else in the document. Publishing
/// such a resource would therefore hand readers a generation they can only
/// part-apply. A resource absent from the held document -- new, or
/// redeclared after a retirement -- is not compared.
///
/// # Errors
///
/// [`PublicationError::ResourceRevisionNotAdvanced`] for the first such
/// resource, data sources first.
pub(crate) fn refuse_unadvanced_resource_revisions(
    snapshot: &RuntimeSnapshot,
    held_tenants: &[TenantBindingDocument],
    held_data_sources: &[DataSourceDocument],
) -> Result<(), PublicationError> {
    refuse_unadvanced(
        DocumentKind::DataSources,
        held_data_sources,
        &snapshot.data_sources.payload,
        |source| (source.id.to_string(), source.revision),
    )?;
    refuse_unadvanced(
        DocumentKind::Tenants,
        held_tenants,
        &snapshot.tenants.payload,
        |tenant| (tenant.tenant.to_string(), tenant.revision),
    )
}

fn refuse_unadvanced<T: Serialize>(
    document: DocumentKind,
    held: &[T],
    offered: &[T],
    identity: impl Fn(&T) -> (String, BindingRevision),
) -> Result<(), PublicationError> {
    let held: HashMap<String, (BindingRevision, &T)> = held
        .iter()
        .map(|resource| {
            let (id, revision) = identity(resource);
            (id, (revision, resource))
        })
        .collect();

    for resource in offered {
        let (id, offered_revision) = identity(resource);
        let Some((held_revision, held_resource)) = held.get(&id) else {
            continue;
        };
        let advanced = offered_revision > *held_revision
            || (offered_revision == *held_revision && same_content(resource, *held_resource));
        if !advanced {
            return Err(PublicationError::ResourceRevisionNotAdvanced {
                document,
                resource: id,
                held: *held_revision,
                offered: offered_revision,
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
