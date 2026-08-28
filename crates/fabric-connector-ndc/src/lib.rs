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
//! implementation of it. So the subset we need is written here, read from
//! version 0.2.13 of the specification but requiring only
//! `NDC_MINIMUM_VERSION` of a connector — the two are different numbers for
//! a reason, and that constant explains it. Both are crate-private: the
//! version is this crate's business, not its callers'.
//!
//! That subset is a **closed list, not a starting point** — see the `wire`
//! module's docs (`src/wire.rs`) for the policy on what happens when NDC can
//! express something this crate does not yet, and why an unsupported
//! capability must fail rather than be approximated.
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
//! Those argument names are configuration, not constants: the specification
//! fixes neither, and both are optional. Naming one in
//! [`NdcConnectorConfig`] is how an operator says this connector routes that
//! way — and it is checked, because a connector is free to ignore a
//! request-level argument it never declared, and the one implementation that
//! can be read does exactly that. See `registration::routing_arguments` for
//! what that costs when nobody checks: every tenant on one database, at `200`.
//!
//! # Operator portability
//!
//! NDC connectors name their own comparison operators (`_eq`, `eq`, `equals` —
//! it varies), but the `/schema` response declares each one's *semantics*.
//! `SchemaIndex` reads that at startup and builds the mapping, so the platform
//! never hardcodes a vendor's operator spelling. An operator the connector does
//! not declare is refused rather than guessed at.

mod client;
mod config;
mod connector;
mod logging;
mod registration;
mod routing;
#[cfg(test)]
mod routing_tests;
mod schema_index;
mod translate;
mod wire;

pub use config::{CollectionProcedures, NdcConnectorConfig, ProcedureBinding};
pub use connector::NdcConnector;
pub use registration::build_ndc_connector;
// Not `pub`. `SchemaIndex` holds the connector's own operator vocabulary --
// the raw spellings it chose for `_eq` and friends -- which is an NDC concept
// through and through, and ADR 0001 puts NDC concepts inside this crate.
// Nothing outside it referenced this, so exporting it bought no caller
// anything and cost the boundary its only structural guarantee.
//
// `NdcConnector::schema_index()` went with it. It existed "for diagnostics"
// and had no caller; an accessor kept alive for a diagnostics surface nobody
// has built is speculative API, and adding it back the day that surface
// exists is a two-line change.
pub(crate) use schema_index::SchemaIndex;

/// The minimum NDC specification version this client requires.
///
/// Sent on every request in the `X-Hasura-NDC-Version` header, and the floor
/// checked against the connector's `/capabilities` response at startup. It is
/// deliberately one value used for both: `versioning.md` defines compatibility
/// as `^{requested-version}`, so the version advertised in the header *is* the
/// contract, and advertising one number while accepting another is a promise
/// this crate would not be keeping.
///
/// # Why 0.2.4 rather than the newest version read
///
/// The wire types here were hand-written against 0.2.13, but 0.2.4 is the
/// version whose features this client actually depends on — it added
/// request-level arguments, which carry every tenant's connection routing.
/// Everything 0.2.5 through 0.2.13 added is relational-query and aggregate
/// surface the Data API never asks for. `versioning.md` asks a client to send
/// "the minimum non-breaking version of the specification that is supported by
/// the client, so that the widest range of connectors can be used", and this is
/// that value. It is also what lets a real connector through: `ndc-postgres`
/// v3.1.0 pins `ndc-models` at v0.2.4 and reports `0.2.4`.
///
/// A connector below this floor, or on a different minor, is rejected at boot
/// rather than discovered through wrong answers under load — see
/// `registration::version` for the full reasoning, including why a matching
/// minor was never sufficient on its own.
pub(crate) const NDC_MINIMUM_VERSION: &str = "0.2.4";

/// The header carrying [`NDC_MINIMUM_VERSION`].
pub(crate) const NDC_VERSION_HEADER: &str = "X-Hasura-NDC-Version";

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
pub(crate) const DOMAIN_ID: u32 = 3;
