//! The Data API — the abstraction boundary between business logic and tenant
//! data infrastructure.
//!
//! # The contract
//!
//! An application asks for a **logical resource**:
//!
//! ```http
//! POST /data/customers
//! Authorization: Bearer <token>
//!
//! {"name": "Alice", "email": "alice@example.com"}
//! ```
//!
//! It does not name a tenant. It does not name a database. It does not hold a
//! connection string, know a placement class, or care which isolation model it
//! is getting (§2, §26). Everything after the resource name is the platform's
//! problem.
//!
//! # What one request actually does
//!
//! ```text
//! POST /data/customers
//!   → tenant_id from the bearer token         (fabric-identity, §10)
//!   → runtime binding for that tenant         (fabric-tenant-runtime, §7)
//!   → catalogue: "customers" → data source + collection   (§15)
//!   → binding: data source → ExecutionTarget  (§16, §17)
//!   → connector executes                      (fabric-connector)
//! ```
//!
//! Each arrow is a lookup in memory. Nothing in that chain reads Git, queries
//! Kubernetes, or opens a connection (§6).
//!
//! # Two things kept deliberately apart
//!
//! **Tenant resolution** answers "which tenant's resources does this target?"
//! **Authorization** answers "may this identity do this?" (§23).
//!
//! They meet nowhere. Authorization can refuse an operation; it can never
//! change which tenant is selected. The tenant comes from the token and only
//! from the token, so there is no policy, role, or scope that can move a
//! request to a different tenant's data.
//!
//! # Failing closed
//!
//! Every way this can go wrong rejects the request (§28). No default tenant, no
//! first-available database, no shared fallback connection, and no tenant
//! selection through a request header (§11).

mod authorization;
mod catalog;
mod config;
mod errors;
mod execution;
mod extraction;
mod handlers;
mod limits;
mod logging;
mod models;
mod registration;
mod request_id;
mod routes;
mod state;

pub use authorization::{OperationKind, ResourcePermissions};
pub use catalog::{ResourceCatalog, ResourceDefinition};
pub use config::DataApiConfig;
pub use errors::DataApiError;
// Not `pub`. `build_data_api` is the only supported way in, because it is
// the only path that validates the configuration and refuses an empty
// catalogue or connector registry first. A caller who could reach the
// service or assemble the state directly could skip all of that.
pub(crate) use execution::DataApiService;
pub use models::{ListQuery, ListResponse, PagingInfo, RowResponse, WriteResponse};
pub use registration::build_data_api;
// `API_PREFIX` stays public: the host needs it to reason about where this
// router mounts, and a test asserts on it. `data_routes` does not, for the
// reason above.
pub(crate) use routes::data_routes;
pub use routes::API_PREFIX;
pub(crate) use state::DataApiState;

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
pub(crate) const DOMAIN_ID: u32 = 4;
