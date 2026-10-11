//! Whether a catalogue names only logical data sources its generation's
//! tenants bind (G8).

use fabric_core::{LogicalDataSourceName, LogicalResourceName};

use crate::{CatalogDocument, PublicationError, TenantBindingDocument};

/// Every catalogue entry whose logical data source no tenant in `tenants`
/// binds, in catalogue order. Empty when `tenants` is: a generation with no
/// tenant serves no request, so no entry in it can be reached.
///
/// ADR 0018 part 4 still allows an entry that *some* tenant does not bind;
/// that tenant gets `UnboundDataSource` on that resource alone. An entry
/// that *no* tenant binds is different: it can only be a catalogue from
/// another generation, or a release published before any placement for it.
#[must_use]
pub fn unbound_catalogue_entries(
    catalog: &CatalogDocument,
    tenants: &[TenantBindingDocument],
) -> Vec<(LogicalResourceName, LogicalDataSourceName)> {
    if tenants.is_empty() {
        return Vec::new();
    }
    catalog
        .resources()
        .filter(|(_, definition)| {
            !tenants.iter().any(|tenant| {
                tenant
                    .data
                    .iter()
                    .any(|(logical, _)| *logical == definition.data_source)
            })
        })
        .map(|(resource, definition)| (resource.clone(), definition.data_source.clone()))
        .collect()
}

/// Refuses a catalogue with an entry [`unbound_catalogue_entries`] names.
///
/// # Errors
///
/// [`PublicationError::CatalogueEntryUnbound`] for the first such entry.
pub(crate) fn refuse_unbound_catalogue_entries(
    catalog: &CatalogDocument,
    tenants: &[TenantBindingDocument],
) -> Result<(), PublicationError> {
    match unbound_catalogue_entries(catalog, tenants).into_iter().next() {
        Some((resource, data_source)) => Err(PublicationError::CatalogueEntryUnbound {
            resource,
            data_source,
        }),
        None => Ok(()),
    }
}
