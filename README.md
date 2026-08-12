# SaaS Fabric

**Tenant-aware infrastructure. Tenant-agnostic applications.**

A GitOps-driven SaaS control plane and tenant runtime that maps an established
tenant identity onto logical platform services, and resolves those services to
tenant-specific physical infrastructure.

This repository implements the **runtime plane** and the **Data API**. The full
architecture is specified in
[docs/architecture/tenant-runtime-data-api.md](docs/architecture/tenant-runtime-data-api.md);
section references throughout the code (§7, §18, §28…) point there.

## The contract

An application asks for a logical resource:

```http
POST /data/customers
Authorization: Bearer <token>

{"name": "Alice", "email": "alice@example.com"}
```

It does not name a tenant. It does not name a database. It never holds a
connection string, never learns a placement class, and cannot tell which
isolation model it is getting.

The platform resolves all of that:

```
POST /data/customers
  → tenant_id from the bearer token            fabric-identity      §10
  → runtime binding for that tenant            fabric-tenant-runtime §7
  → catalogue: customers → data source         fabric-data-api      §15
  → binding: data source → ExecutionTarget     fabric-tenant-runtime §16
  → connector executes                         fabric-connector-ndc
```

Every arrow is an in-memory lookup. Nothing in that chain reads Git, queries
Kubernetes, or opens a connection (§6).

## Crates

| Crate | Role |
|---|---|
| [`fabric-core`](crates/fabric-core) | Shared kernel: validated identifiers, event IDs, the clock seam. No I/O. |
| [`fabric-identity`](crates/fabric-identity) | Bearer token → tenant identity context. Not authentication. |
| [`fabric-tenant-runtime`](crates/fabric-tenant-runtime) | What each tenant *currently has*. Revisioned, lock-free, fail-closed. |
| [`fabric-connector`](crates/fabric-connector) | The neutral execution boundary. No protocol or database types. |
| [`fabric-connector-ndc`](crates/fabric-connector-ndc) | Speaks Hasura NDC v0.2.13. The only crate that knows NDC exists. |
| [`fabric-data-api`](crates/fabric-data-api) | The public HTTP surface. |
| [`fabric-api`](crates/fabric-api) | The composition root. |

Each crate carries `docs/README.md` (for developers) and `docs/CONTEXT.md` (a
summary that avoids reading every file).

## The five ideas worth knowing

### 1. Desired state is not runtime state

Git says what a tenant *should* have. The runtime registry holds what it *does*
have, in memory, written ahead of time by reconciliation. `resolve()` is an
atomic pointer load and a hash lookup — no I/O, no locks, no control-plane
dependency on the request path.

### 2. The tenant comes from the token, and only from the token

There is no code path that reads a tenant from a header. A request carrying
`X-Tenant-Id` is rejected outright rather than quietly ignored, so a caller who
believes the header works finds out immediately (§11).

`TenantIdentity` is an axum extractor, so a handler cannot run without a
resolved tenant. "Did we remember to check the tenant?" is a compile-time
question.

### 3. Isolation is applied in exactly one place

Three placements are supported (§18): a dedicated database, a per-tenant schema,
and a shared table with a discriminator. The first two isolate *structurally* —
the connection cannot see other tenants. The third isolates **only** because the
platform adds a predicate.

So `QuerySpec::for_target` and `MutationSpec::for_target` are the single point
where that predicate is produced, every route to a connector goes through them,
and writes get the discriminator *stamped* onto rows so a caller cannot insert
into another tenant.

### 4. Fail closed, and distinguish the failures

No default tenant, no first-available database, no shared fallback connection
(§28). And the distinctions matter: an unprimed registry is `503`, not "unknown
tenant" — telling every caller during a cold start that their tenant was deleted
would be both wrong and alarming.

### 5. Never widen an operation

If a backend cannot express part of a request, it is **refused**, not
approximated. Silently dropping a predicate is merely wrong in a single-tenant
app; here the dropped predicate might be the tenant boundary, and the failure
looks exactly like success — rows come back, status 200, nothing logged.

## Data execution: NDC as a protocol, not a product

Rather than writing a query engine per database dialect, the platform delegates
to connector processes over the Hasura **Native Data Connector** protocol.
Per-tenant routing rides on NDC request-level arguments (spec 0.2.4).

Two constraints shaped how:

- **`fabric-connector` contains no NDC types.** NDC is an internal boundary,
  swappable for a native provider without touching anything above it.
- **No Hasura code is in the build graph.** `hasura/ndc-spec`, which publishes
  the `ndc-models` protocol crate, carries **no licence at all** — no LICENSE
  file anywhere in the repository, no `license` field, no README statement — so
  it cannot enter an Apache-2.0 platform. The wire types are hand-written from
  the published specification instead. Connector *processes* are consumed over
  HTTP and never linked; `ndc-postgres` v3.1.0 is Apache-2.0.

The reasoning, the licence audit, and the consequences (including what happens
to §22 connection pooling) are recorded in
[ADR 0001](docs/decisions/0001-ndc-as-connector-boundary.md).

## Running it

```bash
cargo run -p fabric-api -- examples/config.toml
```

The example configuration, catalogue, and bindings in [`examples/`](examples)
are covered by tests, so they cannot drift from the code.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
```

Per-domain log levels come free from the crate layout — note the underscores:

```bash
RUST_LOG=info,fabric_tenant_runtime=debug,fabric_connector_ndc=trace
```

## Status

The runtime plane and Data API are implemented and tested. Not yet built:

- The Configuration, Feature, Storage, Events, and Secrets APIs (§27). The
  binding format already carries their state, so adding them does not change the
  tenant model.
- Reconciliation itself. The runtime reads bindings a controller writes; writing
  that controller is a separate piece of work, and the file-backed
  `BindingSource` is the contract between them.
- A JWKS refresher. `VerificationKeys` is a snapshot; rotation currently means
  rebuilding the reader.

## Licence

Apache-2.0. Dependencies are held to OSI-approved licences, verified per crate
and per version — see ADR 0001 for how that is done and why it matters.
