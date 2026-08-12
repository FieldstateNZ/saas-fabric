//! An NDC-speaking implementation of [`DataConnector`](fabric_connector::DataConnector).
//!
//! # Scope
//!
//! This crate is the *only* place in the workspace that knows the NDC protocol
//! exists. Everything above it speaks the neutral vocabulary of
//! `fabric-connector`, which is what keeps NDC replaceable by a native provider
//! later. See [ADR 0001](../../../docs/decisions/0001-ndc-as-connector-boundary.md).
//!
//! **The public SaaS Fabric API is not the NDC API.** Nothing in this crate is
//! re-exported upward, and the Data API's request and response shapes are
//! unrelated to the types here.
//!
//! # Why the wire types are hand-written
//!
//! Hasura publishes `ndc-models`, a crate containing exactly these types. We do
//! not use it, because `hasura/ndc-spec` carries **no licence at all** — no
//! `LICENSE` file anywhere in the repository, no `license` field in its
//! manifests, no statement in its README. Absent a grant, the default is all
//! rights reserved, which cannot enter an Apache-2.0 platform.
//!
//! The specification itself is published, and implementing a published protocol
//! for interoperability is a different act from incorporating someone's
//! implementation of it. So the subset we need is written here, from the
//! specification, and pinned to [`NDC_VERSION`].
//!
//! Connector *processes* are a separate question and a much easier one: they
//! are consumed over HTTP, never linked. `ndc-postgres` v3.1.0 is Apache-2.0.
//! Verify any new connector's licence before adopting it — the licence of one
//! Hasura repository says nothing about the next, as the case above shows.
//!
//! # How multi-tenancy maps onto the protocol
//!
//! Connectors are usually configured with one connection at startup, which is
//! the opposite of what per-tenant placement needs. NDC solves this with
//! **request-level arguments** (added in spec 0.2.4), carried on every query and
//! mutation and intended for exactly this: values that apply to the whole
//! request rather than to a collection.
//!
//! So the tenant's [`ExecutionTarget`](fabric_connector::ExecutionTarget)
//! becomes a `request_arguments` entry:
//!
//! | Placement | Argument | Value |
//! |---|---|---|
//! | Shared server | `connection_name` | The connection's stable name |
//! | Dedicated database | `connection_string` | Assembled from a resolved secret |
//!
//! Named routing is strongly preferred — it keeps the credential inside the
//! connector's own configuration instead of putting it in a request body.
//!
//! # Operator portability
//!
//! NDC connectors name their own comparison operators (`_eq`, `eq`, `equals` —
//! it varies), but the `/schema` response declares each one's *semantics*.
//! [`SchemaIndex`] reads that at startup and builds the mapping, so the platform
//! never hardcodes a vendor's operator spelling. An operator the connector does
//! not declare is refused rather than guessed at.

mod client;
mod config;
mod connector;
mod logging;
mod registration;
mod routing;
mod schema_index;
mod translate;
mod wire;

pub use config::{CollectionProcedures, NdcConnectorConfig, ProcedureBinding};
pub use connector::NdcConnector;
pub use registration::build_ndc_connector;
pub use schema_index::{SchemaIndex, SemanticOperator};

/// The NDC specification version this client implements.
///
/// Sent on every request in the `X-Hasura-NDC-Version` header and checked
/// against the connector's `/capabilities` response at startup. Pinned rather
/// than floating: our wire types are hand-written, so a connector speaking a
/// version we have not read is a mismatch we want to hear about at boot rather
/// than discover through a malformed response under load.
pub const NDC_VERSION: &str = "0.2.13";

/// The header carrying [`NDC_VERSION`].
pub const NDC_VERSION_HEADER: &str = "X-Hasura-NDC-Version";

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
pub(crate) const DOMAIN_ID: u32 = 3;
