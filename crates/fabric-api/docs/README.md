# fabric-api

The SaaS Fabric runtime plane host — the composition root that wires every
runtime-plane domain crate into one process, and the binary that serves it.

Runtime-plane crate, and the runtime plane's composition root (see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md)):
it is the only crate allowed to depend on every other runtime-plane crate at
once, because assembling them is its entire job. Depends on `fabric-core`,
`fabric-identity`, `fabric-tenant-runtime`, `fabric-connector`,
`fabric-connector-ndc`, `fabric-data-api`.

## Why this crate exists

Every other runtime-plane crate does one thing — resolve identity, hold
reconciled state, execute a protocol, serve the Data API — and knows nothing
about the others. Something has to load configuration, decide the security
posture, negotiate connectors, build the catalogue, and serve the result over
one HTTP listener with liveness and readiness probes. That is this crate, and
`main.rs` stays deliberately thin: almost everything it might do instead
lives in `lib.rs`'s modules, where an integration test can build the same
graph the binary builds and assert on it directly — most pointedly, the
shipped example configuration and state in `examples/` are *proven* to load
by `tests/example_configuration.rs` and `tests/example_state.rs`, not merely
believed to.

There is no dependency-injection container and no registry macro anywhere in
this crate. The whole application graph is one function,
`startup::application::build`, readable top to bottom.

## Key concepts

- **The application graph (`startup/application.rs`'s `build`)** — three
  ordered steps: (1) identity — `build_identity` runs
  `IdentityConfig::validate`, so a deployment missing
  `[identity].trusted_issuers` fails at startup rather than on the first
  request (ADR 0019 §2); (2) runtime state — `build_runtime` starts the
  background refreshers for the two independently-reconciled files
  (`tenants.json`, `data-sources.json`); (3) everything else
  (`startup/serving.rs`'s `build`), which negotiates connectors, builds the
  catalogue, builds the Data API, and composes the served router. From step 2
  onward a failure has something to clean up — every failure path stops the
  refreshers before returning (`startup/background_tasks.rs`), because a
  caller embedding `build` (a test, or any future host) cannot rely on "the
  process is about to exit anyway" the way the binary can.
- **Partial connector failure is tolerated; total failure is not**
  (`startup/connectors.rs`, §35). A connector that cannot be negotiated at
  startup is registered anyway, as a `PendingConnector` under its own id —
  every request routed to it gets `ConnectorError::Unreachable` rather than
  `UnknownConnector`, which would look exactly like a tenant binding naming a
  connector nobody configured. A background retry loop
  (`startup/connectors/retry.rs`) keeps attempting it on
  `connector_retry_interval_seconds`; every clone of the `ConnectorRegistry`
  shares the same `Arc<PendingConnector>`, so a later successful retry is
  visible to every holder's next lookup with no restart. The process refuses
  to start only when **zero** connectors negotiated at all — a replica that
  can serve nothing no matter what should never come up.
- **Three independent timeout scopes** (`AppConfig::request_timeout_seconds`'s
  own doc comment) — the overall Data API request budget (this crate,
  outermost), the HTTP call to a connector
  (`NdcConnectorConfig::http_timeout_seconds`, per connector), and database
  execution inside the connector process (not visible to Fabric at all).
  `AppConfig::validate` enforces `request_timeout_seconds >=` the longest
  configured connector timeout — otherwise the outer budget always expires
  first, turning the connector's own clearer timeout into a dead letter.
- **Readiness is a real decision, not "did it load"** (`health/`).
  `registries_can_serve` treats *primed-and-empty* as a legitimate ready
  state for a fresh deployment, but treats **primed tenants with zero
  DataSources as not-ready** (§28) — the asymmetric case that used to answer
  `200` to every request while every one of them 500'd.
  `connectors_can_serve` tolerates partial connector failure by design
  (§35): ready unless *every* connector has definitively answered unhealthy;
  a connector whose check has not answered inside the sweep's budget counts
  as `unknown`, not `down`, because treating "no answer yet" as "answer is
  bad" would let one slow backend pull an otherwise-healthy replica out of
  rotation.
- **`/ready`'s detail is authorised, its verdict is not** (`health/detail_access.rs`).
  `/ready` shares a router with `/v1/data`, on the port applications reach —
  so it used to leak connector ids, estate size and raw upstream error text
  to anyone who could reach the port at all. The status code (what an
  orchestrator needs) stays public; the body's *detail* requires the same
  administrator-role token the Data API itself recognises. A kubelet never
  parses the body, so nothing about the readiness contract changes for it.
- **Configuration is one struct, validated in layers.** `AppConfig` combines
  every domain's own config type; `AppConfig::load` merges a TOML file with
  `FABRIC_SETTING_*` environment overrides (deliberately **not** the whole
  process's `FABRIC_*` namespace — see `config/env_namespace.rs`);
  `AppConfig::validate` runs the cross-cutting checks no single domain crate
  can see for itself (connector uniqueness, distinct state paths, a named
  administrator role, the token/identity issuer-list agreement, the timeout
  relationship above).

## How the pieces fit

```text
main.rs                     load config, validate, startup::build, serve, shut down
startup::build                1. build_identity            (fabric-identity)
                               2. build_runtime              (fabric-tenant-runtime)
                               3. startup::serving::build:
                                    - connectors::build       (fabric-connector-ndc, §35)
                                    - catalog::load
                                    - build_data_api          (fabric-data-api)
                                    - health_routes
                                    - startup::compose         merge, not nest (see below)
config::AppConfig             one struct; loading + validation split into their own modules
health::*                     liveness (nothing external) vs. readiness (§28, §35, §34)
secrets::EnvSecretResolver    development / already-projected-environment SecretResolver
```

`startup::compose` **merges** the Data API's router rather than nesting it
under a prefix — `fabric-data-api` already carries its whole external path
(`/v1/data/...`, its own `API_PREFIX`), and nesting a `/data` prefix on top
would silently produce `/data/v1/...`. The request timeout is applied to the
Data API's router *only*; a `/ready` wrapped in the same timeout could answer
`504`, which a kubelet records as a failed probe — exactly what the
partial-failure readiness policy exists to avoid.

## Getting started

```text
fabric-api /etc/fabric/config.toml
# or
FABRIC_CONFIG=/etc/fabric/config.toml fabric-api
```

See [`examples/config.toml`](../../../examples/config.toml) for a complete,
tested configuration and the three reconciled state files it expects
(`tenants.json`, `data-sources.json`, `catalog.json`), all under `examples/`.
`tests/example_configuration.rs` and `tests/example_state.rs` are what
guarantee that example has not drifted from the code.

## Common tasks

- **Adding a new runtime-plane API crate** (Configuration, Feature, Storage,
  Events, Experience — see
  `docs/architecture/crate-dependencies.md`'s "Adding a crate") — it depends
  on `fabric-core`, `fabric-identity`, `fabric-tenant-runtime`,
  `fabric-connector`, and is composed here, alongside `fabric-data-api`, in
  `startup::serving`'s `router`. It must not depend on `fabric-data-api` or
  any sibling API crate directly.
- **Changing the identity posture** — `[token]` in configuration
  (`TokenConfig::TrustedIngress {}` — the canonical posture — or
  `TokenConfig::Validating { jwks_path, issuers, audiences }` for
  defence-in-depth). Read `config/token_config.rs`'s note on why
  `TrustedIngress` is `{}` and not a bare unit variant before touching this
  enum.
- **Diagnosing a readiness failure** — `GET /ready` with the configured
  administrator role's bearer token; the body names which registry is
  unprimed/empty or which connectors are unhealthy. Without that token, only
  the status code is available (by design).
- **Adding a connector** — append to `[[connectors]]`
  (`NdcConnectorConfig`); `AppConfig::validate` requires distinct ids and at
  least one entry.

## What this crate deliberately does not do

- **No dependency edge to `fabric-fga-auth`.** Nothing in `fabric-data-api`
  calls the Fabric authorization front door yet, and `config/validation/issuers.rs`'s
  own module docs explain why the audience-equality obligation ADR 0019 §1
  states between the Data API and an `IssuerRegistration` cannot be checked
  in this process at all: this crate has no edge to the crate that owns
  `IssuerRegistration`, and the obligation is a platform-level one instead.
- **No re-validation of what the edge already validated**, under the
  canonical `TrustedIngress` posture — see ADR 0002 and ADR 0019 §8/§9. That
  is the trusted-ingress model's whole point, not an oversight.
- **No query to Git or Kubernetes on the request path.** `tenants_path`,
  `data_sources_path` and `catalog_path` are local files a controller writes;
  nothing here reaches out for them (specification §6).

## Tests

- `tests/composed_surface.rs` — builds the router through the real
  `fabric_api::startup::compose`, so this test cannot drift from production
  the way a hand-assembled comparison router once did (missing
  `TimeoutLayer`, missing probe routes, missing `TraceLayer` — the gap that
  let an unbounded `/ready` through review).
- `tests/example_configuration.rs` / `tests/example_state.rs` — load and
  cross-validate the shipped `examples/` configuration and state files
  (every tenant binding's DataSource exists, every DataSource's connector is
  configured, and so on).
- `tests/startup_teardown.rs` — proves a failed `startup::build` leaves no
  background task running, using a configuration whose connectors point at
  a closed port so negotiation fails immediately.
- `tests/readiness_probe.rs` — proves three properties independently: the
  verdict reflects registry *content*, not just "loaded"; the probe answers
  inside a kubelet's ~1s budget even against a blackholed connector; and the
  estate detail is inaccessible without the administrator token.

## Gotchas

- The crate is `fabric-api` (hyphen), but its library target is explicitly
  named `fabric_api` in `Cargo.toml`'s `[lib]` — matching the usual
  hyphen-to-underscore rule exactly, just spelled out rather than left
  implicit. The `RUST_LOG` filter target is `fabric_api`:
  `RUST_LOG=info,fabric_api=debug`.
- `FABRIC_SETTING_*` is the environment namespace for configuration, **not**
  `FABRIC_*`. The latter would also swallow `FABRIC_CONFIG` (the file path),
  every `FABRIC_SECRET_*` variable (`secrets::EnvSecretResolver`), and any
  `FABRIC_API_PORT`/`FABRIC_API_SERVICE_HOST` Kubernetes injects for a
  `Service` named `fabric-*` — all three were real, silently-fatal startup
  failures before the namespace was narrowed (`config/env_namespace.rs`).
- `AppConfig::load` refuses a missing or non-file configuration path
  *outright*, rather than letting `figment` treat it as an empty provider —
  the earlier behaviour silently reverted every setting (including the
  identity posture) to its default on a `volumeMount` typo.
- An `Allowlist` (used for `[token].issuers`/`audiences` under `Validating`)
  cannot be constructed empty. `issuers = []` used to be read as "no
  allowlist" (accept every issuer); now it is a startup failure, and `None`
  (the field omitted) is the only way to mean "do not examine this claim".
- `permissions.administrator_role` cannot be blank. A blank role means
  different things to two different consumers — this crate's own
  `may_see_detail` reads it as "authorise nobody" (fail closed), but
  `fabric-data-api`'s `ResourcePermissions::permits` reads
  `identity.has_role("")` as true for any token carrying an empty `roles`
  entry, i.e. every operation on every resource. Refusing the blank value at
  startup is what makes that disagreement unreachable.
- `PendingConnector::capabilities()`/`schema()` are always the empty,
  supports-nothing value, even after the connector resolves — nothing in
  this codebase reads them through the trait object today; the real answer
  lives on the connector now installed behind `query`/`mutate`/`health`.
