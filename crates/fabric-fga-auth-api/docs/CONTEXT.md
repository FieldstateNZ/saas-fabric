# fabric-fga-auth-api — LLM context

The composition root and binary hosting `fabric-fga-auth` beside an OpenFGA
process it starts, supervises and alone can reach. Runtime-plane crate (see
`docs/architecture/crate-dependencies.md`). Depends on `fabric-core`,
`fabric-fga-auth`, plus `axum`, `figment`, `reqwest`, `serde`, `tokio`
(`macros`, `process`, `rt-multi-thread`, `signal`, `time`),
`tracing`/`tracing-subscriber`. Dev-dependency: `jsonwebtoken` (for
`tests/example_configuration.rs`, which asserts a loaded `Algorithm`).

## Public surface (all `pub mod` from `lib.rs`; no `pub use` re-exports)

- `config::AppConfig { listen: String, embedded: Embedded, issuers:
  Vec<fabric_fga_auth::IssuerRegistration> }` (`serde::Deserialize`,
  `deny_unknown_fields`, all fields `pub`). `AppConfig::load(path: &str) ->
  Result<Self, String>` — Figment: TOML file, then `FABRIC_FGA_SETTING_*` env
  (`__`-nested) overrides.
- `config::CONFIG_PATH_VAR: &str = "FABRIC_FGA_CONFIG"`.
- `config::Embedded { port: u16, binary: String, start_timeout_seconds: u64
  (default 30, `default_start_timeout`), datastore: Option<Datastore> }`
  (`deny_unknown_fields`, all fields `pub`). `port` is always bound to
  `127.0.0.1` by `embedded::start` — there is no field for the host.
- `config::Datastore { engine: String, uri: String }` (`deny_unknown_fields`,
  fields `pub`) — hand-written `Debug` renders `uri` as the literal string
  `"redacted"`.
- `embedded::start(config: &Embedded) -> Result<tokio::process::Child, String>`
  (async) — sends one throwaway `GET /healthz` probe before spawning
  anything (to pay the HTTP client's first-request TLS-trust-store cost
  ahead of the readiness race; result discarded), then spawns `{binary} run
  --http-addr 127.0.0.1:{port} --grpc-addr 127.0.0.1:{port+1} --authn-method
  none [--datastore-engine E --datastore-uri U]` with `kill_on_drop(true)`,
  then races `child.wait()` against a `GET /healthz` poll every 250ms
  (`wait_until_ready`/`poll_until_ready`, both private) until
  `start_timeout_seconds` elapses.
- `startup::build(config: &AppConfig) -> Result<axum::Router, String>`
  (sync) — builds `Registry::build(config.issuers.clone())`,
  `KeyCache::new(Arc::new(HttpKeySource::new()?), Arc::new(SystemClock))`,
  `Verifier`, `OpenFgaDecisions::on_loopback(config.embedded.port)`, `Check`,
  and returns `RuntimeSurface::new(...).router()`.
- `telemetry::init()` — JSON `tracing_subscriber::fmt()`, `EnvFilter` from
  `RUST_LOG` (default `"info"`), installed with `try_init()` (never panics
  if already installed).
- `main.rs` (binary only, not part of the library surface) — `main()` calls
  `telemetry::init()`, resolves the config path (first CLI arg, else
  `FABRIC_FGA_CONFIG`, else `/etc/fabric/authorization.toml`), then `run(path)`.
  `run(config_path: &str) -> Result<(), String>` (private): loads config,
  warns if `embedded.datastore` is `None`, calls `embedded::start` (before
  `startup::build`, deliberately — a front that came up without it would
  answer `503` to everything while looking healthy), builds the router,
  binds `config.listen`, then `tokio::select!`s among: `axum::serve(...)`,
  `service.wait()` (the OpenFGA child exiting), and `shutdown()` (which
  awaits `tokio::signal::ctrl_c()`). Exits non-zero (never panics) on any
  startup failure.

## Internal structure

- `src/config.rs` — the three structs above plus `default_start_timeout()`
  (private, returns `30`).
- `src/embedded.rs` — `start`, `datastore_args` (private, returns the CLI
  arg pairs or `vec![]`), `wait_until_ready`, `poll_until_ready` (both
  private). All child-process management; no production code outside this
  module spawns a process.
- `src/startup.rs` — the one function wiring `fabric-fga-auth`'s pieces.
- `src/telemetry.rs` — `init()`.
- `src/main.rs` — the binary; `run(config_path) -> Result<(), String>`,
  `shutdown()`.

## Hard invariants — do not break

1. **This process exits if the OpenFGA child exits, for any reason.** The
   `tokio::select!` in `main::run` treats the child's exit as fatal, never
   as something to log and continue past; `embedded::start`'s own
   `wait_until_ready` race applies the same rule during startup.
2. **OpenFGA is always started with `--http-addr 127.0.0.1:{port}` and
   `--grpc-addr 127.0.0.1:{port+1}`.** No configuration path can move either
   off loopback — this is what makes `--authn-method none` safe rather than
   an open decision endpoint.
3. **A missing `[embedded.datastore]` logs a `warn` at startup, every
   startup.** Silence there would make "every store, model and tuple lost on
   restart" a fact nobody was told.
4. **`Datastore::uri` is never logged, printed, or included in an error.**
   Passed to the child as a CLI argument, and nowhere else.
5. **Every `AppConfig`/`Embedded`/`Datastore` field set is closed
   (`deny_unknown_fields`).**

## Notes

- `tests/supervision.rs` covers two startup-failure shapes end-to-end
  (unspawnable binary; child that exits immediately, detected via the
  `tokio::select!` race rather than waited out — asserted to complete in a
  small fraction of a 300s window) plus a unit test that `Datastore`'s
  hand-written `Debug` never renders a credential embedded in `uri`. The
  third shape the module docs describe — a child that starts but never
  answers `/healthz` before `start_timeout_seconds` — is implemented
  (`poll_until_ready`'s deadline check) but is **not** exercised by an
  automated test in this crate as of this writing.
- `tests/example_configuration.rs` loads
  `examples/authorization.toml` (repository root) via
  `AppConfig::load` and asserts every field (the JWKS address by its realm
  prefix and `/certs` suffix, so no Keycloak path vocabulary sits in the
  test), including `issuers[0].algorithms
  == vec![Algorithm::RS256]` — this is why `jsonwebtoken` is a
  dev-dependency. This is what would catch a field rename or a value edit to
  the shipped example drifting from what it claims to demonstrate.
- The `authorization-front` Docker build stage (in the repository-root
  `Dockerfile`) packages this crate's binary beside an OpenFGA binary
  (`openfga/openfga:v1.19.0`, copied in as `/usr/local/bin/openfga`) in one
  image, with `FABRIC_FGA_CONFIG=/etc/fabric/authorization.toml` and only
  port `8080` exposed. As of this writing that stage is **not** among the
  images `.github/workflows/release.yml` publishes (`runtime-api`,
  `control-plane-api`, `console` only), and neither `fabric-data-api` nor
  `fabric-api` depends on `fabric-fga-auth`, so nothing in the shipped
  runtime plane calls the front door this crate hosts.
- `embedded::start`'s pre-spawn health probe result is discarded
  (`let _ = probe.get(...).await`) on purpose — it exists only to force the
  HTTP client's expensive first-request setup (TLS trust store load, ~15s
  measured on a developer machine vs. ~298µs for a second request) to happen
  before the readiness race begins, not to observe anything.
