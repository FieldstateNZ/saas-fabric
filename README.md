# SaaS Fabric

**Tenant-aware infrastructure. Tenant-agnostic applications.**

A GitOps-driven SaaS control plane and tenant runtime that maps an established
tenant identity onto logical platform services, and resolves those services to
tenant-specific physical infrastructure.

This repository implements two planes of one product:

- the **runtime plane** — tenant identity, the tenant registry, the Data API —
  which serves every tenant's applications and never reads Git;
- the first slice of the **control plane** — the operator console, the
  Control Plane API, Git desired state, and reconciliation into Keycloak.

The full architecture is specified in
[docs/architecture/tenant-runtime-data-api.md](docs/architecture/tenant-runtime-data-api.md);
section references throughout the code (§7, §18, §28…) point there. The control
plane's own architecture is
[docs/architecture/control-plane.md](docs/architecture/control-plane.md).

```text
                         saas-fabric

       ┌─────────────────────────────────────────┐
       │              CONTROL PLANE              │
       │  React console                          │
       │      ↓                                  │
       │  Control Plane API                      │
       │      ↓                                  │
       │  Client desired state → Git             │
       │      ↓                                  │
       │  Reconciliation → Keycloak adapter      │
       └─────────────────────────────────────────┘

       ┌─────────────────────────────────────────┐
       │              RUNTIME PLANE              │
       │  trusted identity → tenant runtime      │
       │      → Data API → connector             │
       └─────────────────────────────────────────┘
```

The two planes share exactly one crate, and
[`scripts/check_architecture.py`](scripts/check_architecture.py) fails the build
if either depends on the other. They face different networks, fail
independently, and authenticate different things.

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

**Shared kernel**

| Crate | Role |
|---|---|
| [`fabric-core`](crates/fabric-core) | Validated identifiers, event IDs, the clock seam. No I/O. The only crate both planes share. |

**Runtime plane**

| Crate | Role |
|---|---|
| [`fabric-identity`](crates/fabric-identity) | Bearer token → tenant identity context. Not authentication. |
| [`fabric-tenant-runtime`](crates/fabric-tenant-runtime) | Tenant bindings and DataSources. Revisioned, lock-free, fail-closed. |
| [`fabric-connector`](crates/fabric-connector) | The neutral execution boundary. No protocol or database types. |
| [`fabric-connector-ndc`](crates/fabric-connector-ndc) | Speaks Hasura NDC — wire types read from v0.2.13, requires 0.2.4 or newer. The only crate that knows NDC exists. |
| [`fabric-data-api`](crates/fabric-data-api) | The public HTTP surface. |
| [`fabric-api`](crates/fabric-api) | The runtime plane's composition root. |

**Control plane**

| Crate | Role |
|---|---|
| [`fabric-client-model`](crates/fabric-client-model) | What a client is, and the declarative document that says so. No I/O. |
| [`fabric-reconciliation`](crates/fabric-reconciliation) | Comparison and convergence. Owns the identity-provider port; owns no protocol. |
| [`fabric-control-plane`](crates/fabric-control-plane) | The operator-facing API, the desired-state port, and the operator identity seam. |
| [`fabric-keycloak`](crates/fabric-keycloak) | The Keycloak adapter. The only crate that knows Keycloak exists. |
| [`fabric-client-git`](crates/fabric-client-git) | The Git-backed desired-state repository. The only crate that knows Git exists. |
| [`fabric-control-plane-api`](crates/fabric-control-plane-api) | The control plane's composition root. |

**Applications**

| Application | Role |
|---|---|
| [`control-plane-ui`](apps/control-plane-ui) | The React operator console. Talks to the control-plane API and nothing else. |

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

### 6. Operators write desired state; reconciliation makes it true

The control plane never calls a platform service on an operator's behalf. A
change to a client's identity writes a document to Git, and reconciliation
converges Keycloak onto it — so Git is the authority rather than one of two
writers racing (ADR 0008).

The visible consequence is that a successful write reports `pending`, not
`applied`. Writing the document and converging the provider are different events
that fail independently, and an API that reported them as one would be lying
about the second one every time. Reconciliation is idempotent and **only adds**:
it creates what is missing and corrects what it manages, and deletes nothing.

Because it observes before it acts, a realm changed outside SaaS Fabric is
reported as `drifted` rather than silently corrected — the one signal that says
something else is editing the realms the platform owns.

### 7. Keycloak stops at its adapter, exactly as NDC does

`RealmRepresentation` and the admin token exist in `fabric-keycloak` and
nowhere else; a blob hash and a commit exist in `fabric-client-git` and nowhere
else. `scripts/check_architecture.py` fails the build if either vocabulary
escapes.

The operator console says Client, Identity, and Domains. It never says the name
of the service underneath, and there is no workflow anywhere that redirects an
operator into one.

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
| [0008](docs/decisions/0008-desired-state-is-the-authority.md) | Control-plane mutations write desired state; platform services are reconciliation targets |
| [0009](docs/decisions/0009-operator-identity-is-not-tenant-identity.md) | Operator identity is separate from tenant identity |

## Running it

The runtime plane:

```bash
cargo run -p fabric-api -- examples/config.toml
```

The control plane, with development adapters — no cluster, no Keycloak, no
GitHub token:

```bash
cargo run -p fabric-control-plane-api -- examples/control-plane.toml
```

And the operator console against it:

```bash
npm install --prefix apps/control-plane-ui
npm run dev --prefix apps/control-plane-ui
```

Every example in [`examples/`](examples) — both configurations, the catalogue,
the tenant bindings, the DataSources, and the client documents — is covered by
tests, so none of them can drift from the code.

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
- The console gets the same treatment in its own language: `any`, floating
  promises and unused code are errors, and the same 150-line limit applies.

## Status

**Runtime plane:** implemented and tested — tenant identity, the tenant
registry, the Data API, the connector boundary, and the NDC connector.

**Control plane:** the first capability, **client identity**, is implemented and
tested end to end. An operator lists clients and edits a client's realm roles in
the console; the change is written to a client document in Git with optimistic
concurrency; reconciliation converges a Keycloak realm onto it; and the console
shows whether that has actually happened.

Not yet built:

- The Configuration, Feature, Storage, Events, and Secrets APIs (§27). The
  binding format already carries their state.
- **Runtime binding publication.** The runtime reads tenant bindings and
  DataSources that a controller writes; the file-backed `JsonFileSource` is the
  contract between them, and publishing into it is a reconciliation target
  beside Keycloak rather than a control-plane mutation — see
  [the control-plane architecture](docs/architecture/control-plane.md#runtime-publication-boundary).
- **The other platform capabilities**: authorization (OpenFGA), secrets
  (OpenBao), routing (Envoy), observability (Grafana). Each follows the shape
  identity established.
- **Client creation and deletion.** Creating a client is a workflow — routing,
  data placement, secrets, a database — and this increment is the identity slice
  of it. Deletion needs its own confirmation semantics (ADR 0008).
- **Operator authentication beyond a trusted network boundary.** The posture is
  the runtime plane's, and carries the same obligation (ADR 0009).
- A JWKS refresher. `VerificationKeys` is a snapshot, so rotation in the opt-in
  defence-in-depth mode means rebuilding the reader.

## Licence

Apache-2.0. Dependencies are held to OSI-approved licences, verified per crate
and per version — see ADR 0001 for how that is done and why it matters.

## CI

Every push and pull request runs, for the Rust workspace,
`cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo doc`,
`cargo test --workspace`, `cargo deny check`, a file-size check and an
architecture check; and for the operator console, `eslint`, `tsc -b`, `vitest`
and `vite build` ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

Three of those are policy rather than tooling defaults:

- [`deny.toml`](deny.toml) mechanically enforces
  [`docs/architecture/dependency-policy.md`](docs/architecture/dependency-policy.md) —
  approved licences only, no unlicensed crates, no unexpected registries or
  git sources.
- [`scripts/check_file_sizes.py`](scripts/check_file_sizes.py) enforces
  [`docs/architecture/file-size-policy.md`](docs/architecture/file-size-policy.md) —
  production `.rs` files over 150 lines fail the build unless the script's
  exemption list documents why. The console's ESLint configuration applies the
  same limit to its own source.
- [`scripts/check_architecture.py`](scripts/check_architecture.py) enforces the
  invariants no unit test can catch, because the violation is the code
  compiling: NDC types outside their crate, Keycloak representations or
  Git-hosting details outside theirs, an edge between the two planes, a database
  driver or Git library anywhere in the graph, `X-Tenant-Id` being read, a
  dependency the documented graph does not allow, or the operator console naming
  another platform service's API.
