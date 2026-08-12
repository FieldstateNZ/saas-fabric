//! DataSources — the physical data destinations tenants are bound to.
//!
//! # Why these are first-class
//!
//! A DataSource is a real thing that exists whether or not any tenant is using
//! it: a database server, a cluster, a schema-hosting instance. It is
//! provisioned, has a region, has capacity, is monitored, is patched, and is
//! retired. Several tenants share one — that sharing is the point, and it is
//! what keeps connection counts bounded (§22).
//!
//! Modelling it as a resource in its own right rather than as fields repeated
//! inside every tenant's binding means:
//!
//! - **Reuse.** Two hundred tenants on `shared-postgres-02` reference it; they
//!   do not each carry a copy of its endpoint and pool settings.
//! - **Independent reconciliation.** Changing a DataSource's pool size does not
//!   rewrite two hundred tenant records, and does not bump two hundred tenant
//!   revisions.
//! - **Observability.** "What is the state of `sql-au-east-03`?" has an answer,
//!   rather than being reconstructed from whichever tenants happen to mention
//!   it.
//! - **One place to be wrong.** A corrected endpoint is corrected once.
//!
//! Tenant configuration is then left with what is genuinely tenant-specific:
//! which DataSource a logical name is bound to, and how that tenant's data is
//! isolated within it.
//!
//! ```text
//! tenant → logical binding (primary) → DataSource → connector → infrastructure
//! ```

mod capabilities;
mod data_source_resource;
mod placement_class;
mod pool_settings;
mod registry_resource;
mod residency;

pub use capabilities::DataSourceCapabilities;
pub use data_source_resource::DataSource;
pub use placement_class::PlacementClass;
pub use pool_settings::PoolSettings;
pub use residency::DataResidency;
