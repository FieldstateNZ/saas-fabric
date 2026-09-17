# fabric-api — LLM context

The runtime plane's composition root and binary. Runtime-plane crate, and
the one crate in that plane allowed to depend on every other runtime-plane
crate (see `docs/architecture/crate-dependencies.md`). Depends on
`fabric-core`, `fabric-identity`, `fabric-tenant-runtime`, `fabric-connector`,
`fabric-connector-ndc`, `fabric-data-api`, plus `async-trait`, `axum`, `figment`, `http`,
`serde`, `serde_json`, `tokio` (`macros`, `rt-multi-thread`, `signal`),
`tower-http` (`timeout`), `tracing`/`tracing-subscriber`. Dev-dependency:
`tower` (`util`). Library target name is `fabric_api` (declared explicitly in
`Cargo.toml`'s `[lib]`, matching the package's hyphen-to-underscore
identifier).

**No dependency on `fabric-fga-auth`, deliberately** — see
`config/validation/issuers.rs`'s module docs. Nothing in `fabric-data-api`
calls the Fabric authorization front door yet.

## Public surface (all `pub mod` from `lib.rs`: `config`, `health`, `secrets`,
`startup`, `telemetry`)

- `config::AppConfig` — the whole process configuration
  (`serde::Deserialize`, `deny_unknown_fields`, struct-level `default`, all
  fields `pub`). Fields: `listen: String`, `identity: IdentityConfig`,
  `token: TokenConfig`, `leeway: LeewaySeconds`, `tenant_runtime:
  RuntimeConfig`, `tenants_path: PathBuf`, `data_sources_path: PathBuf`,
  `catalog_path: PathBuf`, `data_api: DataApiConfig`, `permissions:
  ResourcePermissions`, `connectors: Vec<NdcConnectorConfig>`,
  `connector_retry_interval_seconds: u64` (default 30),
  `request_timeout_seconds: u64` (default 30, `3x` the connector default of
  10s). `AppConfig::load(path: &str) -> Result<Self, String>`;
  `AppConfig::validate(&self) -> Result<(), String>`.
- `config::TokenConfig` — `#[serde(tag = "mode", rename_all = "snake_case",
  deny_unknown_fields)]`. `TrustedIngress {}` (default; struct-shaped *unit*
  variant, deliberately — `deny_unknown_fields` does not bind on an
  internally-tagged unit variant, so `TrustedIngress` without the braces
  would silently accept `jwks_path`/`issuers`/`audiences` alongside `mode =
  "trusted_ingress"` and discard them with no diagnostic) | `Validating {
  jwks_path: PathBuf, issuers: Option<Allowlist>, audiences:
  Option<Allowlist> }` (both `#[serde(default)]`). `.mode_name() ->
  &'static str` (`"trusted_ingress"` | `"validating"`).
- `config::Allowlist` — non-empty `Vec<String>` newtype (private field);
  `try_new(values: Vec<String>) -> Result<Self, String>` refuses an empty
  list or any blank entry; only reachable through `Deserialize`, which calls
  `try_new`, so `issuers = []` is a startup failure, not "no allowlist".
  `as_slice(&self) -> &[String]`.
- `config::CONFIG_PATH_VAR: &str = "FABRIC_CONFIG"`.
- `health::health_routes(state: HealthState) -> axum::Router` — `GET
  /health` (liveness), `GET /ready` (readiness).
- `health::HealthState { runtime: Arc<RuntimeResolver>, connectors:
  ConnectorRegistry, identity: Arc<IdentityResolver>, administrator_role:
  String }` (`Clone`, all fields `pub`).
- `secrets::EnvSecretResolver` — implements `fabric_connector::SecretResolver`
  by reading `FABRIC_SECRET_<SANITISED_REFERENCE>` from the process
  environment (every non-alphanumeric-ASCII character in the reference
  becomes `_`, uppercased; not injective — `a/b` and `a-b` collide, called
  out deliberately). `PREFIX = "FABRIC_SECRET_"` is `pub(crate)`, checked
  against the settings namespace in `config/env_namespace.rs`'s tests.
- `startup::build(config: &AppConfig) -> Result<Application, String>`
  (async) — the whole application graph.
- `startup::Application { router: Router, listen: String, tasks:
  BackgroundTasks }` (fields `pub`).
- `startup::BackgroundTasks` — owns a `RuntimeHandles` + a
  `ConnectorRetryHandle`; `async fn shutdown(self)` stops both, logging (at
  `warn`, not propagating) either panicking.
- `startup::compose(data: Router, health: Router, request_timeout: Duration)
  -> Router` — `pub` specifically so `tests/composed_surface.rs` builds the
  *real* composition rather than a hand-assembled copy. Merges (not nests)
  `data` wrapped in `TimeoutLayer::with_status_code(GATEWAY_TIMEOUT,
  request_timeout)` with `health`, then layers `TraceLayer::new_for_http()`
  over the whole thing.
- `startup::ConnectorRetryHandle` — `async fn shutdown(self) ->
  Result<(), tokio::task::JoinError>`.
- `startup::shutdown_signal() -> impl Future<Output = ()>` — resolves on
  `SIGINT` (any platform) or (Unix only) `SIGTERM`; on a non-Unix platform
  without `SIGTERM`, or if installing the Unix signal handler fails, that
  branch becomes `pending()` and only `SIGINT` can end it (logging a `warn`
  first in the handler-install-failure case).
- `telemetry::init()` — JSON `tracing_subscriber` (via the `registry()` +
  `EnvFilter` + `fmt::layer().json().with_current_span(true)` composition),
  `RUST_LOG`-driven filter (default `"info"`).

## Internal structure

- `main.rs` — the binary: reads `CONFIG_PATH_VAR`/argv[1] (default
  `/etc/fabric/config.toml`), calls `AppConfig::load` + `validate` +
  `startup::build`, binds, serves with `with_graceful_shutdown(shutdown_signal())`,
  then `application.tasks.shutdown().await` after `axum::serve` returns.
  Logs at `error` and exits non-zero on any failure — never panics.
- `config/` — `app_config.rs` (the struct + `Default`), `token_config.rs`,
  `allowlist.rs`, `administrator_role.rs` (`pub(super) fn validate`, refuses
  a blank `permissions.administrator_role` — see invariant 3 below),
  `env_namespace.rs` (`ENV_PREFIX = "FABRIC_SETTING_"` and `ENV_NESTING =
  "__"`, both `pub(super)`; `CONFIG_PATH_VAR`, `pub`), `loading.rs`
  (`AppConfig::load`, `require_readable_file` — uses `Toml::file_exact`, not
  `Toml::file`, so a relative path never walks upward through parent
  directories looking for a match), `load_failure.rs` (`pub(super) fn
  describe` — attributes a load error to the file or the environment by
  inspecting `figment::Error`'s `Source::File` metadata, not by guessing),
  `validation.rs` (`AppConfig::validate` and its own
  `validate_connectors`/`validate_timeouts`/`validate_state_paths`) +
  `validation/issuers.rs` (`pub(super) fn validate(token, identity)` — the
  one relationship neither `TokenConfig` nor `IdentityConfig` can see for
  itself).
- `health/` — `connector_health.rs` (`ConnectorOutcome { id, health:
  ConnectorHealth }`, `ConnectorHealth { Healthy | Unhealthy(String) |
  Unknown }`, all `pub(super)`), `connector_sweep.rs` (`pub(super) const
  HEALTH_BUDGET: Duration = Duration::from_millis(500)`; `pub(super) async
  fn sweep` — concurrent `JoinSet` over every registered connector, collects
  until the budget expires, treats a panicking check as `Unhealthy`, leaves
  anything still running as `Unknown`), `detail_access.rs` (`pub(super) fn
  may_see_detail(state, headers) -> bool` — `false` if
  `administrator_role.is_empty()`, defence in depth even though
  `config::administrator_role` already refuses that at startup, because
  `HealthState` is constructible without going through `AppConfig`),
  `logging.rs` (`pub(super) fn connectors_swept`, logs at `debug` per
  non-healthy connector), `probes.rs` (`pub(super) async fn liveness() ->
  StatusCode` always `OK`; `pub(super) async fn readiness(...)` — runs the
  sweep, builds `RegistryFacts`/`ConnectorFacts`, calls `is_ready`/`is_degraded`,
  renders `readiness_body::detailed` or `::minimal` depending on
  `may_see_detail`), `readiness_body.rs` (`pub(super) fn minimal(ready) ->
  Value` = `{"ready": ready}`; `pub(super) fn detailed(...)` adds
  `degraded`, `tenants_primed`, `data_sources_primed`, `tenants`,
  `data_sources`, and a `connectors` array each with `id`/`status` and
  `reason` only when unhealthy), `readiness_facts.rs` (`RegistryFacts {
  primed: bool, count: usize }`, `ConnectorFacts { total, healthy, unknown:
  usize }`, `impl From<&[ConnectorOutcome]> for ConnectorFacts`),
  `readiness_state.rs` (pure `const fn`s: `is_ready`,
  `registries_can_serve(tenants, data_sources) -> bool` = `tenants.primed &&
  data_sources.primed && (tenants.count == 0 || data_sources.count > 0)`,
  `connectors_can_serve(connectors) -> bool` = `connectors.total == 0 ||
  connectors.healthy > 0 || connectors.unknown > 0`, `is_degraded(connectors)`
  = `connectors.healthy < connectors.total`), `routes.rs` (`health_routes`),
  `state.rs` (`HealthState`).
- `secrets.rs` — `EnvSecretResolver`.
- `startup/` — `application.rs` (`build`, `Application`),
  `background_tasks.rs` (`BackgroundTasks`, `pub(super) async fn
  stop_refreshers`, `pub(super) async fn stop_connector_retry` — both log a
  `warn` on a panicked task rather than propagating), `catalog.rs`
  (`pub(super) fn load(path: &Path) -> Result<ResourceCatalog, String>`, a
  plain synchronous file read — the catalogue is platform-level and
  identical for every tenant, not reconciled state), `compose.rs`
  (`compose`), `connectors.rs` + `connectors/{logging,negotiate,
  negotiation_failure,pending_connector,pending_connector_delegation,retry}.rs`
  (§35's whole implementation — see Hard invariants), `serving.rs`
  (`pub(super) async fn build` — negotiates connectors, then calls the
  private `router` fn which builds the Data API and health routes and
  composes them; stops the connector retry loop if `router` fails),
  `shutdown.rs` (`shutdown_signal`), `token_reader.rs` (`pub(super) fn
  build(config: &TokenConfig, leeway) -> Result<Arc<dyn TokenReader>,
  String>` — builds `TrustedIngressReader` or `ValidatingReader` per
  `TokenConfig`; only the `Validating` arm can fail, reading a JWKS file
  once, never fetching).
- `telemetry.rs` — `init()`.

### `startup::connectors` submodule detail

- `logging.rs` — `negotiation_failed` (`error`), `retry_failed` (`warn`),
  `connector_recovered` (`info`), all `pub(super)`.
- `negotiate.rs` — `pub(super) async fn negotiate(config, secrets) ->
  Result<(ConnectorRegistry, Vec<PendingRetry>), String>` — attempts every
  `[[connectors]]` entry via `build_ndc_connector`; a failure is logged and
  installed as a `PendingConnector` (never left unregistered); returns `Err`
  only when zero connectors negotiated.
- `negotiation_failure.rs` — `NegotiationFailure(String)`, a `pub(super)`
  newtype implementing `std::error::Error` so a negotiation failure message
  can travel as `ConnectorError::Unreachable`'s `source`.
- `pending_connector.rs` — `PendingConnector { id: ConnectorId, resolved:
  RwLock<Option<Arc<dyn DataConnector>>>, reason: RwLock<String>,
  capabilities: ConnectorCapabilities (all-false/empty), schema:
  ConnectorSchema::default() }`. `new(id, reason) -> Arc<Self>`; `resolve(&self,
  connector)`; `record_failure(&self, reason)`; `id()`, `capabilities()`,
  `schema()` (all `const fn`, `pub(super)`); `resolved_connector(&self) ->
  Option<Arc<dyn DataConnector>>` (clones out and drops the guard before any
  `.await`); `unavailable(&self) -> ConnectorError` — always
  `ConnectorError::Unreachable { connector: self.id.clone(), source:
  Box::new(NegotiationFailure(reason)) }`. Uses `std::sync::RwLock`
  (`PoisonError::into_inner` on a poisoned lock), not `tokio::sync`, because
  the critical section is a pointer clone with no `.await` inside it.
- `pending_connector_delegation.rs` — `impl DataConnector for
  PendingConnector`: `query`/`mutate`/`health` each check
  `resolved_connector()` and either delegate or return `unavailable()`.
- `retry.rs` + `retry/retry_handle.rs` — `PendingRetry { config:
  NdcConnectorConfig, placeholder: Arc<PendingConnector> }`; `pub(super) fn
  spawn(pending, secrets, interval) -> ConnectorRetryHandle` — a
  `tokio::spawn`ed loop that, when `pending` is non-empty, sleeps `interval`
  (racing a `Notify` for shutdown) then calls `retry_once`, which retries
  every still-pending connector and removes any that now negotiate; when
  `pending` starts (or becomes) empty, the loop awaits the shutdown
  `Notify` and exits without ever ticking again. `ConnectorRetryHandle {
  shutdown: Arc<Notify>, task: JoinHandle<()> }`; `shutdown(self) ->
  Result<(), JoinError>` notifies then awaits the task.

## Hard invariants — do not break

1. **§35 partial connector failure**: startup fails only when **zero**
   connectors negotiate. A connector that fails is registered as a
   `PendingConnector` under its own id (never left unregistered — an
   unregistered id would look exactly like `UnknownConnector` to a caller),
   retried on `connector_retry_interval_seconds`, and every clone of
   `ConnectorRegistry` observes a successful retry through the shared
   `Arc<PendingConnector>` with no restart.
2. **`AppConfig::request_timeout_seconds` must be `>=` the longest
   configured connector `http_timeout_seconds`**, checked at startup
   (`validate_timeouts`) — otherwise the outer budget always fires first and
   the connector's own timeout becomes unreachable.
3. **`permissions.administrator_role` cannot be blank.** A blank value is
   read as "authorise nobody" by this crate's `may_see_detail` but as
   "match every token with an empty role" by
   `fabric_data_api::ResourcePermissions::permits` (`identity.has_role("")`)
   — a live privilege escalation if it ever reached that far. Refusing it at
   startup is the fix available in *this* crate.
4. **`FABRIC_SETTING_*` is the only settings namespace**; `FABRIC_CONFIG`
   and `FABRIC_SECRET_*` must never fall inside it. This is asserted by
   tests in `config/env_namespace.rs`, not just documented.
5. **`readiness_state::registries_can_serve` requires both registries primed,
   and (tenants-primed-nonempty implies DataSources-primed-nonempty)** —
   "primed" alone is not sufficient; primed tenants with zero DataSources is
   `not ready`, because every `resolve_data_source` in that state is a
   non-retryable 500.
6. **A leak-free background-task teardown on every `build` failure path.**
   `startup::application::build` stops the refreshers on any failure from
   step 3 onward (`background_tasks::stop_refreshers`) — a caller other
   than the binary (a test, an embedding host) must never be left with
   orphaned tasks just because "the process is about to exit anyway"
   happens to be true for the binary.
7. **`/ready`'s status code is always public; its body's estate detail
   requires the administrator-role token.** A kubelet reads only the status
   code, so this is free from the orchestrator's point of view.
8. **`startup::compose` merges the Data API router rather than nesting a
   prefix.** `fabric-data-api` carries its whole path (`API_PREFIX`)
   already; nesting would silently double it.

## Notes

- `tests/composed_surface.rs` builds the router through the real
  `fabric_api::startup::compose` specifically so this test cannot drift from
  production the way a hand-assembled comparison router once did (missing
  `TimeoutLayer`, missing probe routes, missing `TraceLayer` — the gap that
  let an unbounded `/ready` through review). No connector negotiation
  happens in it, so its `/v1/data` assertions are routing assertions only:
  401 proves the route exists, 404 proves it does not.
- `tests/example_configuration.rs` / `tests/example_state.rs` load and
  cross-validate the shipped `examples/` configuration and state files
  (every tenant binding's DataSource exists, every DataSource's connector is
  configured, etc.) — an example that has drifted from the code is
  considered worse than no example. `examples/config.toml` ships the
  canonical `trusted_ingress` posture, one connector (`postgres-au-east`),
  and three trusted issuers (`acme`, `globex`, `initech`).
- `tests/startup_teardown.rs` proves a failed `startup::build` leaves no
  background task running, using the example configuration with its
  connectors' `endpoint` repointed at a closed port (`http://127.0.0.1:1`)
  and 1-second connector timeouts so negotiation fails immediately.
- `tests/readiness_probe.rs` proves three properties independently: the
  verdict reflects registry *content*, not just "loaded"; the probe answers
  inside a kubelet's ~1s budget even against a blackholed connector; and the
  estate detail is inaccessible without the administrator token.
