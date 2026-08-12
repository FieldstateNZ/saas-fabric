# ADR 0001 — Adopt NDC as the internal connector boundary, as a protocol only

- **Status:** Accepted
- **Date:** 2026-08-12
- **Applies to:** the data execution layer beneath the Data API
- **Related:** [Tenant Runtime & Data API Platform Specification](../architecture/tenant-runtime-data-api.md) §13–§22

## Context

The Data API needs to execute logical data operations against whatever physical
datastore a tenant is currently bound to. The naive route is to write a query
engine per database — a PostgreSQL one, a SQL Server one, a MySQL one. That is a
large, permanent, low-differentiation maintenance burden: filtering, sorting,
pagination, projection, mutation semantics, type mapping, and schema
introspection, re-solved once per dialect.

Hasura's **Native Data Connector (NDC)** specification already defines exactly
that boundary: an HTTP service that exposes a data source's native capabilities
to a caller that wants to push execution down into it. Connectors already exist
for PostgreSQL and many other sources.

Adopting it raises three questions, which this ADR answers in order:
does it fit the tenancy model, is it licence-clean, and how do we stop it
becoming our public API.

## Decision

**Adopt NDC as an internal connector protocol. Depend on no Hasura code.**

Three parts:

1. `fabric-connector` defines a **neutral** `DataConnector` trait and a neutral
   logical operation model. No NDC type appears anywhere in it.
2. `fabric-connector-ndc` implements that trait by speaking NDC v0.2.13 over
   HTTP, using **hand-written wire types**.
3. Connector processes (for example `ndc-postgres`) are deployed alongside the
   runtime and consumed over the network. They are never linked into our binary.

The SaaS Fabric responsibility chain is unchanged and remains ours:

```
bearer token → tenant_id → TenantRuntimeBinding → logical datasource binding
             → connector selection → execute logical operation
```

## Licence verification

This was the deciding factor, and the result was not what we expected.
Every component was checked individually, at a specific version.

| Component | Version checked | Licence | Verdict |
|---|---|---|---|
| `hasura/ndc-spec` — contains the `ndc-models` crate | `v0.2.13` | **None** | ❌ **Rejected as a dependency** |
| `hasura/ndc-sdk-rs` | `v0.9.0` | Apache-2.0 (`LICENSE`) | ✅ Acceptable, but not needed |
| `hasura/ndc-postgres` | `v3.1.0` (2025-08-01) | Apache-2.0, © 2023 Hasura Inc. (`LICENSE.txt`) | ✅ Acceptable — consumed over HTTP |
| NDC specification documents | 0.2.13 | Published specification | ✅ Implemented for compatibility |

### `ndc-spec` carries no licence at all

The repository that publishes the `ndc-models` protocol crate has:

- no `LICENSE` file at the repository root, on `main` or at tag `v0.2.13`;
- no licence file anywhere in the repository — a GitHub code search for
  `filename:LICENSE` in `hasura/ndc-spec` returns **0** results;
- no `license` field in the workspace `Cargo.toml` or in `ndc-models/Cargo.toml`;
- no licence statement in the README.

Absent an explicit grant, the default position is *all rights reserved*. An
unlicensed dependency cannot be incorporated into an Apache-2.0 platform, so
`ndc-models` is rejected regardless of how convenient it would have been.

This is worth stating plainly because the surrounding ecosystem reads as open
source — the SDK and the PostgreSQL connector genuinely are Apache-2.0. The
protocol-definitions crate is the one that is not, and it is the one we would
have linked against. It is exactly the case the "verify the specific repository,
crate and version" rule exists to catch.

The crates are also absent from crates.io (`ndc-models`, `ndc-sdk`, `ndc-client`
all return no results), so the only route would have been a git dependency —
which additionally makes the workspace unpublishable and pins to a mutable tag.

### What we do instead

Implement the wire format ourselves. Implementing a published protocol for
interoperability is a different act from incorporating its reference
implementation, and it is what the "prefer protocol compatibility over depending
on a larger product" principle asks for anyway.

We hand-write serde types for the subset of NDC we actually use — capabilities,
schema, `QueryRequest`, `MutationRequest`, and their responses — confined to
`fabric-connector-ndc`. The subset is small because our Data API deliberately
exposes far less than NDC can express.

**Re-verify before adopting any new connector.** The licence of one Hasura
repository says nothing about the next, as the table above demonstrates.

## How tenancy maps onto the protocol

The concern with an off-the-shelf connector is that connectors are normally
configured with one connection at startup, which is the opposite of what
per-tenant placement needs.

NDC handles this. `QueryRequest` and `MutationRequest` both carry:

```rust
request_arguments: Option<BTreeMap<ArgumentName, serde_json::Value>>
```

Request-level arguments were added in **NDC 0.2.4**, explicitly for values that
apply to the whole request rather than to one collection — the specification
gives dynamically-changing authentication tokens and connection configuration as
the motivating examples. `ndc-postgres` builds on this with *dynamic
connections*, in two modes:

- **`named`** — `configuration.json` declares a `dynamicSettings` map of named
  connection URIs sourced from environment variables, and the request selects
  one with a `connection_name` request argument.
- **`dynamic`** — the request supplies a `connection_string` directly.

This lines up with the isolation models in §18 of the specification:

| Tenant placement | Mode | Where the value comes from |
|---|---|---|
| Shared server, shared/schema isolation | `named` | The physical data source's stable name, from the runtime binding |
| Dedicated database | `dynamic` | Connection string assembled from the binding plus a `SecretResolver` lookup |

Named mode is the default. Dynamic mode puts a credential in a request body, so
it is used only where a tenant genuinely has its own database, and the value is
excluded from telemetry (§29 forbids secrets in telemetry — enforced by never
placing the assembled string in a tracing field).

## Consequences

### Good

- No per-dialect query engine to write, test, or maintain.
- Every NDC-conformant connector becomes reachable, whoever wrote it.
- Zero Hasura code in the build graph, so no licence exposure and no version
  coupling to their release cadence.
- The connector runs as its own process, so a misbehaving driver cannot take the
  runtime plane down with it.

### Bad, and accepted

- **§22 connection-pool management largely moves out of our process.** This is
  the significant one. Pool sizing, idle eviction, and connection recovery
  become the connector's configuration rather than our code. What remains ours
  is connector-instance lifecycle, HTTP client pooling, and driving credential
  rotation by changing what we send in `request_arguments`. The §22 objective —
  preventing `replicas × tenants` connection growth — still holds, and holds
  better: pools live in a small number of connector processes rather than in
  every application replica. The mechanism is just no longer ours to write.

  Since [ADR 0003](0003-data-sources-are-first-class-resources.md), the intended
  sizing is at least *declared* somewhere the platform can see it:
  `DataSource.pool` states what a given database's pool should be, and
  reconciliation applies it to the connector. That does not move the mechanism
  back into this process, but it does mean a reviewer asking "does §22 hold for
  `shared-postgres-02`?" has one object to look at.
- **We own our wire types.** They can drift from the specification. Mitigated by
  pinning to 0.2.13, asserting the negotiated version against
  `/capabilities` at startup, and keeping the implemented subset small.
- **An extra network hop** per data operation. Acceptable: the connector sits
  next to the database, and pushing the whole operation down means one hop, not
  one per row.
- **Capability variance between connectors.** Not every connector supports every
  predicate. `fabric-connector` therefore models capabilities explicitly and
  fails closed on an unsupported operation rather than silently degrading.

## Alternatives rejected

| Alternative | Why rejected |
|---|---|
| Write native per-database query engines now | The cost this ADR exists to avoid. Still possible later — `DataConnector` is the seam that keeps it possible. |
| Depend on `ndc-models` via git | Unlicensed. Also unpublishable and pinned to a mutable tag. |
| Use `ndc-sdk-rs` | Apache-2.0 and fine, but it is for *building* connectors. We are the caller, not the connector. |
| Adopt Hasura DDN wholesale | Brings a product and its licensing surface where we need a protocol. The engine, routing, and metadata layers duplicate what SaaS Fabric already owns. |
| Expose NDC as the public Data API | Explicitly out of bounds. It would leak physical query semantics to applications and break §2, §13, and §26. |

## Invariants this decision must not break

1. NDC types never appear above `fabric-connector-ndc`. The Data API's public
   contract is unchanged by this ADR.
2. Tenant selection stays derived from the bearer token. `request_arguments`
   carries the *resolved physical target*, never a caller-supplied tenant hint.
3. Applications never learn which connector served a request. Connector identity
   is internal telemetry only (§29).
4. No Hasura code enters the build graph without a new ADR recording a verified
   licence and version.
