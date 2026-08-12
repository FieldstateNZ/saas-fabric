//! The runtime tenant registry — what each tenant currently *has*.
//!
//! # The distinction this crate exists to enforce
//!
//! Git holds **desired** state: what a tenant should have. This crate holds
//! **runtime** state: what it currently does have. Specification §6 is emphatic
//! that these are different things and that only the second one may appear in
//! request handling.
//!
//! ```text
//! Git desired state → reconciliation → Runtime Tenant Registry → requests
//! ```
//!
//! The forbidden shape is a request that reaches sideways for its answer:
//! read tenant → query Git → query Kubernetes → discover database → resolve
//! secret → execute. Every one of those steps is a network call on the request
//! path, and each is a way for a control-plane outage to become a data-plane
//! outage.
//!
//! So reconciliation writes ahead of time, this registry holds the result in
//! memory, and [`TenantRuntimeRegistry::resolve`] is a map lookup behind an
//! atomic pointer read. No I/O, no locks on the read path.
//!
//! # Fail closed
//!
//! [`ResolveError`] has three variants and all of them reject the request
//! (§28). Note especially that "the registry has not loaded yet" is *not* the
//! same as "this tenant does not exist" — the first is
//! [`ResolveError::RuntimeUnavailable`] and becomes a 503, the second is
//! [`ResolveError::UnknownTenant`] and becomes a rejection. Conflating them
//! would tell every caller during a cold start that their tenant had been
//! deleted.
//!
//! There is no default tenant, no first-available database, and no shared
//! fallback connection. Those are named in §28 as things the runtime must never
//! silently do.
//!
//! # Change propagation
//!
//! Bindings are revisioned (§20), and revisions only move forward. Subscribe
//! with [`TenantRuntimeRegistry::subscribe`] to learn when a tenant's binding
//! changes — that is the signal for the layer below to retire whatever it had
//! attached to the old binding, which is what makes a live migration (§19)
//! possible without redeploying applications.

mod binding;
mod change;
mod config;
mod errors;
mod logging;
mod refresher;
mod registration;
mod registry;
mod snapshot;
mod source;
mod sources;

pub use binding::{ConfigurationBinding, DataBinding, StorageBinding, TenantRuntimeBinding};
pub use change::{BindingChange, BindingChangeKind};
pub use config::TenantRuntimeConfig;
pub use errors::{BindingSourceError, ResolveError};
pub use refresher::{BindingRefresher, RefreshHandle};
pub use registration::build_tenant_runtime;
pub use registry::{ApplyReport, TenantRuntimeRegistry};
pub use source::BindingSource;
pub use sources::{FileBindingSource, InMemoryBindingSource};

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
pub(crate) const DOMAIN_ID: u32 = 2;
