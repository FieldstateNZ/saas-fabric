//! Validated identifiers used throughout the runtime plane.
//!
//! Every identifier in this module is a newtype whose only fallible
//! constructor performs a full character-set check. Downstream code that holds
//! one of these types never needs to re-validate, which is what lets the Data
//! API interpolate a tenant's schema name into SQL without a second thought.

mod binding_revision;
mod data_source_id;
mod data_source_name;
mod logical_resource_name;
pub(crate) mod slug;

pub use binding_revision::BindingRevision;
pub use data_source_id::DataSourceId;
pub use data_source_name::DataSourceName;
pub use logical_resource_name::LogicalResourceName;

mod tenant_id;
pub use tenant_id::TenantId;
