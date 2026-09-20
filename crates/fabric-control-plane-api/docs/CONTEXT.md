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
- `config::PlatformManagementConfig { environment, registry, observation,
  reconciliation_interval_seconds, operation_timeout_seconds, publication:
  Option<PublicationConfig> }` -- the whole section optional; a deployment
  that manages a platform but publishes no runtime state simply omits
  `publication`.
- `config::PublicationConfig { namespace, interval_seconds }`
  (`[platform_management.publication]`, ADR 0023 part 4) --
  `deny_unknown_fields`; `interval_seconds` defaults to 60, and 0 disables
  the schedule while leaving `POST /api/platform/publication` working.
  `namespace` is validated at startup against
  `fabric_publication_kubernetes::PublicationTarget::validate`, beside the
  budget check.
- `secrets::{PREFIX, resolve}`.
- `startup::{build, Application, shutdown_signal}`;
  `Application { router, listen, reconciliation }`.
- `startup::platform::{establish, Established, start_sweeping,
  start_publishing}` -- `establish` builds `Established { platform:
  Option<PlatformBinding>, publication: Option<PublicationSink> }`; the
  publisher itself is built later, in `build_control_plane`, from
  `Established::publication` plus the client desired-state binding `establish`
  does not have. `start_publishing` is `start_sweeping`'s sibling: absent
  config or a zero interval spawns nothing, the first tick is immediate, a
  failed pass never stops the loop.
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
