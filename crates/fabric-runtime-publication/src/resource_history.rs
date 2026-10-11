//! The highest revision each tenant binding and DataSource has ever been
//! published at, carried in the publication record.
//!
//! The held documents only say what is published now. A resource that left
//! its document -- a DataSource removed and published, or a tenant taken out
//! by a Git revert -- leaves nothing behind in them, so without this a
//! redeclaration under the same id could restart at revision 1 (O3). A
//! reader that never applied the removal still holds the old revision, would
//! ignore the lower one, and would serve a generation only part-applied.
//! D01-1 requires resource revisions to stay monotonic across that.

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId, TenantId};

use crate::{DataSourceDocument, TenantBindingDocument};

/// For every resource any recorded generation has published, the highest
/// revision it was published at. Entries are never removed.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceHistory {
    /// Tenant bindings, by tenant id.
    pub tenants: BTreeMap<TenantId, BindingRevision>,
    /// DataSources, by id.
    #[serde(rename = "data-sources")]
    pub data_sources: BTreeMap<DataSourceId, BindingRevision>,
}

impl ResourceHistory {
    /// The highest revision `tenant` has been published at, if ever.
    #[must_use]
    pub fn tenant(&self, tenant: &TenantId) -> Option<BindingRevision> {
        self.tenants.get(tenant).copied()
    }

    /// The highest revision `data_source` has been published at, if ever.
    #[must_use]
    pub fn data_source(&self, data_source: &DataSourceId) -> Option<BindingRevision> {
        self.data_sources.get(data_source).copied()
    }

    /// This history, raised to cover every resource in `tenants` and
    /// `data_sources`.
    #[must_use]
    pub fn including(
        mut self,
        tenants: &[TenantBindingDocument],
        data_sources: &[DataSourceDocument],
    ) -> Self {
        for tenant in tenants {
            raise(&mut self.tenants, &tenant.tenant, tenant.revision);
        }
        for data_source in data_sources {
            raise(&mut self.data_sources, &data_source.id, data_source.revision);
        }
        self
    }
}

fn raise<K: Ord + Clone>(history: &mut BTreeMap<K, BindingRevision>, id: &K, revision: BindingRevision) {
    let highest = history.entry(id.clone()).or_insert(revision);
    if revision > *highest {
        *highest = revision;
    }
}
