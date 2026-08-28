# fabric-control-plane-api — LLM context

The control plane's composition root. Depends on every control-plane crate plus
`axum`, `figment`, `serde`, `serde_json`, `tokio`, `tower-http`, `tracing`,
`tracing-subscriber`. Library + binary; the binary is thin so the graph is
testable.

**Depends on no runtime-plane crate**, and the architecture check enforces it.

## Public surface

- `config::ControlPlaneAppConfig { listen, control_plane, desired_state,
  identity_provider, request_timeout_seconds }` + `load(&str)`. **No `Default`.**
- `config::DesiredStateConfig` — `Git(GitRepositoryConfig)` |
  `LocalDirectory { path }`. Tagged `mode`.
- `config::IdentityProviderConfig` — `Keycloak(KeycloakConfig)` | `InMemory`.
  Tagged `mode`.
- `config::CONFIG_PATH_VAR = "FABRIC_CP_CONFIG"`.
- `secrets::{PREFIX, resolve}`.
- `startup::{build, Application, shutdown_signal}`;
  `Application { router, listen, reconciliation }`.
- `telemetry::init`.

## Hard invariants — do not break

1. **The API is never given the identity provider.** `build` hands
   `build_control_plane` the repository and the clock, and nothing else.
2. **`ControlPlaneAppConfig` gains no `Default`.**
3. **Both adapter choices stay tagged enums**, so a development adapter cannot
   be reached by omission.
4. **A development adapter warns loudly at startup**, at `warn`, saying what is
   lost.
5. **`FABRIC_CP_SETTING_` stays disjoint from `FABRIC_SETTING_` and
   `FABRIC_SECRET_`.** There are tests for all three.
6. **`/health` requires no operator; nothing else is exempt.**
7. **The request timeout wraps the API and not the probe.** A probe answering
   `504` is recorded as a failure and the replica pulled.

## Design notes

- `telemetry::init` duplicates fifteen lines from `fabric-api`. Sharing them
  would need either a forbidden dependency on a runtime crate or a shared crate
  with one function in it. The duplication is visible, small, and carries no
  invariant.
- `LocalDirectory` seeds `InMemoryClientRepository` from `*.yaml` at startup;
  writes stay in memory and are lost on restart, which the warning says. A
  document that will not parse fails startup rather than being skipped.
- `tests/example_configuration.rs` loads `examples/control-plane.toml` and
  parses every document in `examples/clients/`, so a renamed field fails the
  build rather than the example silently rotting.
