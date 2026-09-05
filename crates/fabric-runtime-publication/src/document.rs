//! The producer's own declarations of the runtime's wire shapes.
//!
//! Every type here is a **separate declaration** from the consumer's own
//! Rust type — this crate may not depend on `fabric-tenant-runtime`,
//! `fabric-connector`, or `fabric-data-api` (see
//! `docs/architecture/crate-dependencies.md`). Fidelity between the two
//! copies is not a shared type; it is `#[serde(deny_unknown_fields)]` on the
//! consumer's side, plus a round-trip test beside each type here that
//! deserialises this crate's own canonical JSON as the consumer's type.

mod capabilities;
mod catalog;
mod configuration_binding;
mod connection_selector;
mod data_source;
mod isolation;
mod placement;
mod pool_settings;
mod residency;
mod resource_definition;
mod storage_binding;
mod tenant_binding;
mod tenant_data_binding;

pub use capabilities::DataSourceCapabilitiesDocument;
pub use catalog::CatalogDocument;
pub use configuration_binding::ConfigurationBindingDocument;
pub use connection_selector::ConnectionSelectorDocument;
pub use data_source::{data_sources_canonical_json, DataSourceDocument};
pub use isolation::IsolationModelDocument;
pub use placement::PlacementClassDocument;
pub use pool_settings::PoolSettingsDocument;
pub use residency::DataResidencyDocument;
pub use resource_definition::ResourceDefinitionDocument;
pub use storage_binding::StorageBindingDocument;
pub use tenant_binding::{tenants_canonical_json, TenantBindingDocument};
pub use tenant_data_binding::TenantDataBindingDocument;
