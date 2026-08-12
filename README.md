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
connection string, learns a placement class, or discovers which isolation model
it is getting.

## The model

```
tenant → logical binding (primary) → DataSource → connector → infrastructure
```

Two independently reconciled resources:

- A **tenant binding** answers *which DataSource is this tenant's `primary`
  bound to, and how is this tenant isolated within it?* — and nothing else.
- A **DataSource** owns every physical concern: connector, connection, pool
  sizing, placement class, residency, capabilities, observability labels.

They are separate because a DataSource is shared. Two hundred tenants reference
`shared-postgres-02`; correcting its endpoint is one edit to one resource, and
bumps one revision instead of two hundred. See
[ADR 0003](docs/decisions/0003-data-sources-are-first-class-resources.md).

One request:

```
POST /data/customers
  → tenant_id from the bearer token            fabric-identity        §10
  → catalogue: customers → logical data source fabric-data-api        §15
  → tenant binding: logical name → DataSource  fabric-tenant-runtime  §7
  → DataSource: connector + connection         fabric-tenant-runtime  §16
  → connector executes                         fabric-connector-ndc
```

Every arrow is an in-memory lookup. Nothing in that chain reads Git, queries
Kubernetes, or opens a connection (§6).

## Crates

| Crate | Role |
|---|---|
| [`fabric-core`](crates/fabric-core) | Shared kernel: validated identifiers, event IDs, the clock seam. No I/O. |
| [`fabric-identity`](crates/fabric-identity) | Bearer token → tenant identity context. Not authentication. |
| [`fabric-tenant-runtime`](crates/fabric-tenant-runtime) | Tenant bindings and DataSources. Revisioned, lock-free, fail-closed. |
| [`fabric-connector`](crates/fabric-connector) | The neutral execution boundary. No protocol or database types. |
| [`fabric-connector-ndc`](crates/fabric-connector-ndc) | Speaks Hasura NDC v0.2.13. The only crate that knows NDC exists. |
| [`fabric-data-api`](crates/fabric-data-api) | The public HTTP surface. |
| [`fabric-api`](crates/fabric-api) | The composition root. |

Each crate carries `docs/README.md` (for developers) and `docs/CONTEXT.md` (a
summary that avoids reading every file).

## The ideas worth knowing

### 1. Desired state is not runtime state

Git says what a tenant *should* have. The runtime registries hold what it *does*
have, in memory, written ahead of time by reconciliation. Resolution is an
atomic pointer load and two hash lookups — no I/O, no locks, no control-plane
dependency on the request path.

### 2. The tenant comes from the token, and only from the token

There is no code path that reads a tenant from a header. A request carrying
`X-Tenant-Id` is rejected outright rather than quietly ignored (§11).
`TenantIdentity` is an axum extractor, so a handler cannot run without a
resolved tenant — "did we remember to check the tenant?" is a compile-time
question.

**Trusted ingress is the canonical posture.** The gateway authenticates and
validates the bearer; the runtime consumes the identity it established and is
authentication-agnostic (§8, §9, §24). Signature verification is available as
opt-in defence in depth, but it is not the recommended architecture and not the
fix for a missing network boundary — see
[ADR 0002](docs/decisions/0002-trusted-ingress-is-the-canonical-identity-model.md).

### 3. Isolation is applied in exactly one place

Three placements are supported (§18): a dedicated database, a per-tenant schema,
and a shared table with a discriminator. The first two isolate *structurally*.
The third isolates **only** because the platform adds a predicate.

So `QuerySpec::for_target` and `MutationSpec::for_target` are the single point
where that predicate is produced, every route to a connector goes through them,
and writes get the discriminator *stamped* onto rows so a caller cannot insert
into another tenant.

### 4. Fail closed, and distinguish the failures

No default tenant, no first-available database, no shared fallback connection
(§28). And the distinctions matter: an unprimed registry is `503`, not "unknown
tenant" — telling every caller during a cold start that their tenant was deleted
would be both wrong and alarming. A binding naming a DataSource that does not
exist is a `500`, never a silent reroute.

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

The reasoning, the licence audit, and the consequences are recorded in
[ADR 0001](docs/decisions/0001-ndc-as-connector-boundary.md).

## Decisions

| ADR | Subject |
|---|---|
| [0001](docs/decisions/0001-ndc-as-connector-boundary.md) | NDC as the internal connector boundary, as a protocol only |
| [0002](docs/decisions/0002-trusted-ingress-is-the-canonical-identity-model.md) | Trusted ingress is the canonical identity model |
| [0003](docs/decisions/0003-data-sources-are-first-class-resources.md) | DataSources are first-class resources |

## Running it

```bash
cargo run -p fabric-api -- examples/config.toml
```

The example configuration, catalogue, tenant bindings, and DataSources in
[`examples/`](examples) are covered by tests, so they cannot drift from the code.

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets
```

Per-domain log levels come free from the crate layout — note the underscores:

```bash
RUST_LOG=info,fabric_tenant_runtime=debug,fabric_connector_ndc=trace
```

## Conventions

- Small files: roughly 50–80 lines, split by responsibility. Inline tests live
  in sibling `*_tests.rs` modules where a type's tests would otherwise dominate
  its file.
- Strict lints: `unwrap`, `expect`, `panic`, and indexing are **denied** outside
  tests; clippy pedantic is on; `unsafe` is forbidden.
- Every public item carries rustdoc explaining *why*, written for someone who
  does not know the domain yet.

## Status

The runtime plane and Data API are implemented and tested. Not yet built:

- The Configuration, Feature, Storage, Events, and Secrets APIs (§27). The
  binding format already carries their state.
- Reconciliation itself. The runtime reads tenant bindings and DataSources that
  a controller writes; the file-backed `JsonFileSource` is the contract between
  them.
- A JWKS refresher. `VerificationKeys` is a snapshot, so rotation in the opt-in
  defence-in-depth mode means rebuilding the reader.

## Licence

Apache-2.0. Dependencies are held to OSI-approved licences, verified per crate
and per version — see ADR 0001 for how that is done and why it matters.

## CI

Every push and pull request runs `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
`cargo deny check`, and a file-size check
([`.github/workflows/ci.yml`](.github/workflows/ci.yml)). The latter two are
policy, not just tooling defaults:

- [`deny.toml`](deny.toml) mechanically enforces
  [`docs/architecture/dependency-policy.md`](docs/architecture/dependency-policy.md) —
  approved licences only, no unlicensed crates, no unexpected registries or
  git sources.
- [`scripts/check_file_sizes.py`](scripts/check_file_sizes.py) enforces
  [`docs/architecture/file-size-policy.md`](docs/architecture/file-size-policy.md) —
  production `.rs` files over 150 lines fail the build unless the script's
  exemption list documents why.
