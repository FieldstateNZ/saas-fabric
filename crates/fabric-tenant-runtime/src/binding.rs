//! What a tenant currently has.
//!
//! These types are the runtime counterpart to the tenant definition in Git.
//! They differ in one important way: the Git definition expresses *intent*
//! (`class: dedicated`, `region: au-east`), while these express *outcome* (this
//! connector, this connection, this isolation model). Turning the first into
//! the second is reconciliation's job, not the runtime's.

mod configuration_binding;
mod data_binding;
mod storage_binding;
mod tenant_runtime_binding;

pub use configuration_binding::ConfigurationBinding;
pub use data_binding::DataBinding;
pub use storage_binding::StorageBinding;
pub use tenant_runtime_binding::TenantRuntimeBinding;
