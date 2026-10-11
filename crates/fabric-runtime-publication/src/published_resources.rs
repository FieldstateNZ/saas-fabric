//! What a target holds, resource by resource, for a caller that composes
//! against the last publication before offering the next (O3, O4, G8).

use fabric_core::{BindingRevision, DataSourceId, TenantId};

use crate::{CatalogDocument, DataSourceDocument, ResourceHistory, TenantBindingDocument};

/// The held documents, parsed, and the resource history the held record
/// carries, raised to cover them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublishedResources {
    /// The held tenants document; empty when none is held.
    pub tenants: Vec<TenantBindingDocument>,
    /// The held data-sources document; empty when none is held.
    pub data_sources: Vec<DataSourceDocument>,
    /// The held catalogue, when one is held and parses. The plan only ever
    /// byte-compares it, so one that does not parse is `None` here rather
    /// than a refusal.
    pub catalog: Option<CatalogDocument>,
    /// The highest revision each resource has been published at.
    pub history: ResourceHistory,
}

impl PublishedResources {
    /// The held binding for `tenant`, if the tenants document holds one.
    #[must_use]
    pub fn tenant(&self, tenant: &TenantId) -> Option<&TenantBindingDocument> {
        self.tenants.iter().find(|held| held.tenant == *tenant)
    }

    /// The highest revision `data_source` has been published at, held now
    /// or removed since.
    #[must_use]
    pub fn data_source_revision(&self, data_source: &DataSourceId) -> Option<BindingRevision> {
        self.history.data_source(data_source)
    }
}
