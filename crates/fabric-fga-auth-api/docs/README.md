# fabric-fga-auth-api

Hosts the Fabric authorization front (`fabric-fga-auth`) beside an OpenFGA
process it alone can reach. This is the composition root and binary for the
`authorization-front` container image described in
[ADR 0016](../../../docs/decisions/0016-fabric-owns-the-authorization-front-door.md).

Runtime-plane crate (see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md)).
Depends on `fabric-core` and `fabric-fga-auth`.

## Why this crate exists

`fabric-fga-auth` is a library: it verifies tokens, binds identities, and
knows how to ask a `Decisions` port for an answer. Something has to load
configuration, **start the OpenFGA process this front fronts**, wire the
library's pieces together, and bind a socket. That something is this crate —
kept separate from the library the same way `fabric-api` is kept separate
from `fabric-data-api`, so the library stays testable without a process to
launch.

```text
this container
+------------------------------------------------+
|  fabric-fga-auth-api   :8080  (published)      |
|      registry -> verifier -> Check             |
|                    |                           |
|  openfga         127.0.0.1  (not published)    |
+------------------------------------------------+
```

The published address is the *only* way in. OpenFGA is started and
supervised by this process, listens on loopback only, and cannot be
addressed from outside the container — which is what makes running it with
`--authn-method none` a containment rather than a hole
([ADR 0016](../../../docs/decisions/0016-fabric-owns-the-authorization-front-door.md)).

## Key concepts

- **This process owns OpenFGA's lifecycle, and dies with it.** `embedded::start`
  spawns the OpenFGA binary as a child with `kill_on_drop(true)`; `main.rs`'s
  `tokio::select!` races serving the router against `service.wait()` and
  `ctrl_c()` — if OpenFGA exits for *any* reason, this process exits too. The
  alternative (a shell starting both, or two independently-supervised
  processes) leaves the worst case looking healthy: the front keeps serving
  `503` to every decision while its own liveness probe passes, and an
  operator sees a "working" pod that authorizes nothing.
- **Readiness is spent *before* the readiness probe exists to protect.**
  `embedded::start` sends one throwaway health probe *before spawning the
  child at all*, because an HTTP client's first request does one-off
  work — loading the platform's TLS trust store — that can take *seconds* of
  blocking work inside what should be a fast poll. Measured on a developer
  machine: 15s for the first request, 298µs for the second, 165µs for a raw
  TCP connect. Paying that cost up front is what makes the readiness race
  (below) actually a race.
- **Starting the child races against it dying**, rather than polling and
  checking in between: `wait_until_ready` uses `tokio::select!` between
  `child.wait()` and a poll loop (`GET /healthz` every 250ms), so a service
  that dies immediately is noticed as it happens rather than at the next poll
  boundary (measured at up to ~13 seconds late with a naive poll-then-check
  loop).
- **The datastore is optional, and its absence is loud.** `Embedded::datastore:
  Option<Datastore>` — when absent, OpenFGA keeps every store, model and
  tuple in memory and loses all of it on restart. `main.rs` logs a `warn` at
  startup when this is the case; it is a reasonable default for a
  development run and never one for a deployment.
- **`Datastore`'s `Debug` is hand-written** to redact `uri` (which carries a
  connection-string credential) — the derive would have printed a password
  the first time this config reached a log line.

## How the pieces fit

```text
main.rs           loads AppConfig, calls embedded::start, then startup::build,
                  then races serving / the child exiting / shutdown
config.rs         AppConfig { listen, embedded: Embedded, issuers: Vec<IssuerRegistration> }
embedded.rs       spawns and supervises the OpenFGA child process
startup.rs        wires fabric-fga-auth's Registry/KeyCache/Verifier/Check/
                  OpenFgaDecisions into a RuntimeSurface, returns its router
telemetry.rs      JSON tracing subscriber
```

## Getting started

```text
FABRIC_FGA_CONFIG=/etc/fabric/authorization.toml fabric-fga-auth-api
```

or pass the path as the first argument; if neither is given, `main.rs` falls
back to `/etc/fabric/authorization.toml`. The shipped example,
[`examples/authorization.toml`](../../../examples/authorization.toml),
loads under test (`tests/example_configuration.rs`) and looks like this:

```toml
listen = "0.0.0.0:8080"

[embedded]
port = 8088
binary = "/usr/local/bin/openfga"
start_timeout_seconds = 30

# Absent means in memory, lost on restart. Fine for development, never for a
# deployment -- the host warns loudly when this section is missing.
#   [embedded.datastore]
#   engine = "postgres"
#   uri = "postgres://fabric@openfga-db.secrets.svc.cluster.local/openfga"

[[issuers]]
tenant = "acme"
issuer = "https://identity.fabric.example/realms/acme"
audience = "saas-fabric"
jwks_uri = "http://keycloak-http.identity.svc.cluster.local/realms/acme/protocol/openid-connect/certs"
algorithms = ["RS256"]
store = "01ABCDEFGHIJKLMNOPQRSTUVWX"
authorization_model_id = "01ZYXWVUTSRQPONMLKJIHGFEDC"
```

`audience` must equal the Data API's own required audience
(`[token].audiences` in `examples/config.toml`) and the Keycloak client's
audience-mapper value (`[identity_provider].audience` in
`examples/control-plane.toml`) — one string across the deployment (ADR 0019
§1, §G5).

## Common tasks

- **Changing which port OpenFGA listens on** — `embedded.port` in
  configuration; the gRPC port is always `port + 1`. Both are always bound
  to `127.0.0.1` — there is no configuration surface that can change that
  (see `embedded.rs`'s module docs for why it is not a config field at all).
- **Diagnosing a startup failure** — `tests/supervision.rs` exercises two of
  the ways OpenFGA can fail to come up (an unspawnable binary; a child that
  exits immediately) and proves both end the *process*, never leave it
  serving. A third path — the child starting but never answering
  `/healthz` within `embedded.start_timeout_seconds` — is implemented
  (`embedded::poll_until_ready`) but has no dedicated test in this crate as
  of this writing.
- **Wiring a persistent OpenFGA datastore** — add `[embedded.datastore]`
  with `engine` and `uri`. This is OpenFGA's own datastore, not something
  this Rust binary talks to directly (no database driver is linked in this
  crate).

## What this crate deliberately does not do

- **No decision logic of its own.** Every check is answered by
  `fabric-fga-auth`'s `RuntimeSurface`; this crate only loads configuration,
  starts the child process, and wires the router.
- **No way to reach OpenFGA except through the front.** `embedded.port` is
  always bound to `127.0.0.1` by `embedded::start`; there is no config field
  or argument for a host.
- **Not shipped yet.** The `authorization-front` Docker stage (in the
  repository-root `Dockerfile`) is **not** among the images
  `.github/workflows/release.yml` publishes today (`runtime-api`,
  `control-plane-api`, `console` only) — the image builds and its own tests
  pass, but nothing ships it yet, and nothing in `fabric-data-api` or
  `fabric-api` calls the front door this hosts.

## Tests

- `tests/supervision.rs` — proves a service that cannot be spawned, and a
  service that exits immediately (`/usr/bin/true` or `/bin/true`, whichever
  exists), both end `embedded::start` in an `Err` rather than a process left
  serving; also proves `Datastore`'s hand-written `Debug` never renders a
  credential embedded in `uri`.
- `tests/example_configuration.rs` — loads the shipped
  `examples/authorization.toml` through `AppConfig::load` and asserts every
  field against it, so the example and the config schema cannot drift apart
  silently.

## Gotchas

- The crate is `fabric-fga-auth-api` (hyphen); the Rust identifier — and the
  `RUST_LOG` filter target — is `fabric_fga_auth_api` (underscore):
  `RUST_LOG=info,fabric_fga_auth_api=debug`. This process uses a JSON
  tracing subscriber (`telemetry::init`).
- `AppConfig`, `Embedded` and `Datastore` all `deny_unknown_fields` — a
  typo'd setting is a startup failure, not a silently-ignored value.
- `embedded::start`'s throwaway pre-spawn health probe deliberately ignores
  its own result (`let _ = probe.get(...).await`) — it exists purely to pay
  the TLS-trust-store cost before the readiness race begins, not to check
  anything.
- `AppConfig::load` merges `FABRIC_FGA_SETTING_*` environment variables
  (double-underscore nested) over the TOML file, separate from
  `FABRIC_FGA_CONFIG`, which names the file itself and is read only by
  `main.rs`.
