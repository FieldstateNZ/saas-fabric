//! The runtime plane — what each tenant *currently has*.
//!
//! # The model
//!
//! ```text
//! tenant → logical binding (primary) → DataSource → connector → infrastructure
//! ```
//!
//! Two reconciled resources, deliberately separate:
//!
//! - A [`TenantRuntimeBinding`] answers **"which DataSource is this tenant's
//!   `primary` bound to, and how is this tenant isolated within it?"** and
//!   nothing else.
//! - A [`DataSource`] owns every physical and provider concern: connector,
//!   connection, pool sizing, placement class, residency, capabilities,
//!   observability labels.
//!
//! They are separate because a DataSource is shared. Two hundred tenants
//! reference `shared-postgres-02`; they do not each carry a copy of its
//! endpoint. Correcting that endpoint is one edit to one resource, and bumps
//! one revision instead of two hundred. Each is reconciled from its own source,
//! on its own schedule.
//!
//! [`RuntimeResolver`] is the only supported way to walk the chain.
//!
//! # Desired state is not runtime state
//!
//! Git holds what a tenant *should* have; this crate holds what it *does* have.
//! §6 is emphatic that only the second may appear in request handling.
//!
//! ```text
//! Git desired state → reconciliation → runtime registries → requests
//! ```
//!
//! The forbidden shape is a request reaching sideways for its answer: read
//! tenant → query Git → query Kubernetes → discover database → resolve secret →
//! execute. Every step there is a network call on the request path, and each is
//! a way for a control-plane outage to become a data-plane outage.
//!
//! So reconciliation writes ahead of time, the registries hold the result in
//! memory, and resolution is an atomic pointer read and a hash lookup.
//!
//! # Fail closed
//!
//! Every [`ResolveError`] rejects the request (§28). Note especially that "the
//! registry has not loaded yet" is *not* "this tenant does not exist" — the
//! first is [`ResolveError::RuntimeUnavailable`] and becomes a 503, the second
//! is [`ResolveError::UnknownTenant`]. Conflating them would tell every caller
//! during a cold start that their tenant had been deleted.
//!
//! There is no default tenant, no first-available database, and no shared
//! fallback connection.

mod config;
mod data_source;
mod errors;
mod logging;
mod registration;
mod resolution;
mod resource;
mod tenant;
#[cfg(test)]
mod testing;

pub use config::RuntimeConfig;
pub use data_source::{DataResidency, DataSource, DataSourceCapabilities, PlacementClass, PoolSettings};
pub use errors::{ConfigurationError, ResolveError, SourceError};
pub use registration::{build_runtime, RuntimeHandles};
pub use resolution::{ResolvedDataSource, RuntimeResolver};
pub use resource::sources::{InMemorySource, JsonFileSource};
pub use resource::{
    ApplyReport, ChangeKind, LookupError, RefreshHandle, RegistryResource, ResourceChange, ResourceRefresher,
    ResourceRegistry, ResourceSource,
};
pub use tenant::{ConfigurationBinding, StorageBinding, TenantDataBinding, TenantRuntimeBinding};

/// The registry of tenant bindings.
pub type TenantRegistry = ResourceRegistry<TenantRuntimeBinding>;

/// The registry of DataSource resources.
pub type DataSourceRegistry = ResourceRegistry<DataSource>;

/// A change to a tenant's binding.
pub type TenantChange = ResourceChange<fabric_core::TenantId>;

/// A change to a DataSource.
pub type DataSourceChange = ResourceChange<fabric_core::DataSourceId>;

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
pub(crate) const DOMAIN_ID: u32 = 2;
