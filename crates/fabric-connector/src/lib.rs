//! The neutral data-execution boundary.
//!
//! # Why this crate exists
//!
//! Beneath the Data API, *something* has to actually run a query against a real
//! database. We deliberately do not write that something per dialect. Instead
//! the platform delegates to a connector process that already knows how to
//! speak to the datastore — see
//! [ADR 0001](../../../docs/decisions/0001-ndc-as-connector-boundary.md).
//!
//! This crate is the seam that keeps that a *choice*. It defines what a
//! connector is in terms the platform owns:
//!
//! - [`DataConnector`] — the trait every execution backend implements.
//! - [`QuerySpec`] / [`MutationSpec`] — a neutral operation model.
//! - [`Filter`], [`SortField`] — a neutral predicate and ordering AST.
//! - [`ExecutionTarget`] — where a tenant's data physically lives.
//! - [`ConnectorCapabilities`] — what a given backend can actually do.
//!
//! # The rule that makes it work
//!
//! **No protocol-specific or database-specific type may appear in this crate.**
//! No NDC types, no SQL, no driver types, no wire formats. If NDC were replaced
//! tomorrow by a native PostgreSQL provider, everything here would compile
//! unchanged and only `fabric-connector-ndc` would be rewritten.
//!
//! That rule is what the surrounding requirement — "maintain an abstraction
//! that allows NDC to be replaced or supplemented by native providers later" —
//! actually reduces to in code. It is easy to state and easy to violate by
//! accident, so if you find yourself reaching for a wire type here, that is the
//! signal that the abstraction is leaking.
//!
//! # Capabilities, and failing closed
//!
//! Backends differ. A connector for a document store may not support the same
//! predicates as one for PostgreSQL. Rather than silently degrading a query —
//! dropping a filter the backend cannot express and returning too many rows —
//! the platform asks what a connector supports and rejects operations it does
//! not. Silently returning *more* rows than the caller filtered for is a
//! cross-tenant data leak waiting to happen, so this path fails closed (§28).

mod capabilities;
#[cfg(test)]
mod capabilities_tests;
mod connector;
mod errors;
#[cfg(test)]
mod errors_tests;
mod execution;
mod filter;
mod ids;
mod mutation;
mod ordering;
mod query;
mod registry;
#[cfg(test)]
mod registry_tests;
mod row;
mod schema;
mod secret;
#[cfg(test)]
mod secret_tests;
#[cfg(test)]
mod testing;

pub use capabilities::ConnectorCapabilities;
pub use connector::DataConnector;
pub use errors::{ConnectorError, RefusalDetail, UnsupportedFeature};
pub use execution::{ConnectionSelector, ExecutionTarget, IsolationModel};
pub use filter::{ComparisonOperator, Filter};
pub use ids::{CollectionName, ConnectionName, ConnectorId, FieldName, SchemaName};
pub use mutation::{MutationOutcome, MutationSpec};
pub use ordering::{SortDirection, SortField};
pub use query::{QueryOutcome, QuerySpec};
pub use registry::ConnectorRegistry;
pub use row::Row;
pub use schema::{CollectionSchema, ConnectorSchema};
pub use secret::{ResolvedSecret, SecretRef, SecretResolver};
