//! One immutable view of every tenant's bindings.

use std::collections::HashMap;
use std::sync::Arc;

use fabric_core::TenantId;

use crate::TenantRuntimeBinding;

/// An immutable map of every tenant the runtime currently knows about.
///
/// # Why a whole-snapshot swap rather than a locked map
///
/// Resolution happens on every single request, and it must not contend. A
/// `RwLock<HashMap<..>>` would serialise readers against any writer, so a
/// refresh — which touches every tenant — would stall the entire request path
/// for as long as it took.
///
/// Instead the registry builds a fresh snapshot off to one side and swaps a
/// pointer to it. Readers take an atomic load and never block, refreshes never
/// stall a request, and a request that is mid-flight keeps the snapshot it
/// started with rather than seeing a half-applied update.
///
/// The cost is a full rebuild per refresh, which is a copy of a few thousand
/// small `Arc`s — cheaper than one database round trip, and the request path is
/// where the budget matters.
///
/// Bindings are behind `Arc` so cloning one out of the snapshot is a refcount
/// bump, not a deep copy of the tenant's whole binding tree.
#[derive(Debug, Default)]
pub struct RegistrySnapshot {
    tenants: HashMap<TenantId, Arc<TenantRuntimeBinding>>,
}

impl RegistrySnapshot {
    /// Builds a snapshot from bindings.
    #[must_use]
    pub(crate) fn new(tenants: HashMap<TenantId, Arc<TenantRuntimeBinding>>) -> Self {
        Self { tenants }
    }

    /// Looks up a tenant.
    pub(crate) fn get(&self, tenant: &TenantId) -> Option<&Arc<TenantRuntimeBinding>> {
        self.tenants.get(tenant)
    }

    /// Borrows the underlying map, for building the next snapshot from this one.
    pub(crate) const fn tenants(&self) -> &HashMap<TenantId, Arc<TenantRuntimeBinding>> {
        &self.tenants
    }

    /// How many tenants this snapshot holds.
    pub(crate) fn len(&self) -> usize {
        self.tenants.len()
    }
}
