//! What a tenant currently has.
//!
//! These types are the runtime counterpart to the tenant definition in Git.
//! They differ in one important way: the Git definition expresses *intent*
//! (`class: dedicated`, `region: au-east`), while these express *outcome* (this
//! DataSource, this isolation). Turning the first into the second is
//! reconciliation's job, not the runtime's.
//!
//! # What a tenant binding does and does not hold
//!
//! It answers exactly one question per logical name: **which DataSource is this
//! bound to, and how is this tenant isolated within it?**
//!
//! It holds no connector, no endpoint, no pool settings, no region. Those
//! belong to the [`DataSource`](crate::DataSource) and are shared by every
//! tenant using it. Duplicating them per tenant would mean a thousand copies to
//! correct when an endpoint changes, and a thousand revisions bumped to do it.

mod configuration_binding;
mod registry_resource;
mod storage_binding;
mod tenant_data_binding;
mod tenant_runtime_binding;

pub use configuration_binding::ConfigurationBinding;
pub use storage_binding::StorageBinding;
pub use tenant_data_binding::TenantDataBinding;
pub use tenant_runtime_binding::TenantRuntimeBinding;
