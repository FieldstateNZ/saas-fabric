//! How a tenant binding plugs into the generic registry.

use fabric_core::{BindingRevision, TenantId};

use crate::resource::RegistryResource;
use crate::TenantRuntimeBinding;

/// Registry integration for [`TenantRuntimeBinding`].
///
/// Kept apart from the type's own file because it answers a different
/// question. `tenant_runtime_binding.rs` says *what a tenant binding is*; this
/// says *how it participates in the reconciled-resource lifecycle* — what it is
/// keyed by, and which field the revision guard compares.
///
/// Note that the revision here is the *tenant's*, independent of the revision
/// of whichever DataSource it points at. That independence is the point of
/// [ADR 0003](../../../../docs/decisions/0003-data-sources-are-first-class-resources.md).
impl RegistryResource for TenantRuntimeBinding {
    type Key = TenantId;

    const KIND: &'static str = "tenant";

    fn key(&self) -> &Self::Key {
        &self.tenant
    }

    fn revision(&self) -> BindingRevision {
        self.revision
    }
}
