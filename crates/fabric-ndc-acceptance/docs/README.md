# fabric-ndc-acceptance

A test-only crate: the one place a test is allowed to compose the real
`fabric-runtime-publication` publisher, the real `fabric-tenant-runtime`
runtime, the real `fabric-data-api` Data API, and the real
`fabric-connector-ndc` adapter against a running NDC connector process. It
has no production code and ships nothing a caller could depend on.

The composed acceptance test now lives here:
`tests/published_state_reaches_a_real_connector.rs` publishes a fixture
through the real filesystem publisher, brings up `postgres:16-alpine` and
`ghcr.io/hasura/ndc-postgres:v3.1.0` as subprocesses (`tests/support/docker/`),
negotiates the real adapter against the running connector, and drives the
assembled router with `tower::ServiceExt::oneshot`. `tests/the_stack_comes_up.rs`
is the container harness's own proof that it comes up and answers, one layer
below the composed test. `docs/verification.md`'s "Connector acceptance
(issue #62)" section is the fuller account: what each test proves, the
mutation experiment that falsifies "no predicate reached the connector," and
which falsified assumptions (F1, F2, F4) this closed against a real
connector, versus the one (F3) that is out of scope for this crate and
deferred to its own issue.

## Why this crate exists

Three checks in `scripts/check_architecture.py` interact, and between them
they block every other placement for this test. `check_ndc_containment`
scans every `.rs` file in a crate -- `tests/` included, on purpose, because
an integration test can name whatever it likes -- and fails any crate other
than `fabric-connector-ndc` itself (and, narrowly, `fabric-api`) that names
an NDC type; its companion loop fails any crate other than those two that
merely *declares a dependency* on `fabric-connector-ndc`, dev-dependencies
included. Separately, `check_runtime_plane_cannot_reach_the_publisher`
refuses every `RUNTIME_PLANE` crate -- which includes `fabric-connector-ndc`
and `fabric-data-api` -- a dependency edge to `fabric-runtime-publication`
in any table, dev included. Put together: nothing that can see the NDC
crate is allowed to see the publisher, and nothing that can see the
publisher is allowed to see the NDC crate, so no existing crate can host a
test that needs both at once.

The fix is not to widen any of those three gates. `fabric-ndc-acceptance` is
a new crate in **neither plane** -- the same footing as `fabric-core` and
`fabric-runtime-publication` -- so nothing in `RUNTIME_PLANE` or
`CONTROL_PLANE` depends on it, and its own dependency closure never reaches
into either plane's crates from the other side. `scripts/check_architecture.py`
admits it by name, in both loops of `check_ndc_containment`
(`NDC_ACCEPTANCE_CRATE`), rather than relaxing either check for everyone.
See `src/lib.rs` for the full argument, with line numbers, and
`docs/architecture/crate-dependencies.md` for how this crate is recorded in
the dependency graph.

## What lives here

Nothing production-shaped. `Cargo.toml` has no `[dependencies]` section at
all -- only `[dev-dependencies]`, naming the internal crates the composed
test drives (`fabric-runtime-publication`, `fabric-connector-ndc`,
`fabric-tenant-runtime`, `fabric-data-api`, `fabric-identity`,
`fabric-connector`), the transport and async crates that drive them
(`axum`, `http`, `tokio`, `tower`, `serde_json`), and `base64` (decodes this
crate's own unsigned test tokens -- see `tests/support/unsigned_reader.rs`
for why it cannot use `fabric_identity::TrustedIngressReader` directly).
`publish = false`: this crate is never meant to leave the workspace.

`tests/`:

- `published_state_reaches_a_real_connector.rs` -- the composed acceptance
  test (issue #62 slice 4).
- `the_stack_comes_up.rs` -- the container harness's own smoke test (slice 3).
- `fixtures/ndc-postgres-v3.1.0/` -- the CLI-generated `configuration.json`
  for both connector modes, checked in with the regeneration command. The
  static one is that CLI output verbatim; the named one starts from the same
  output with its `dynamicSettings` block added by hand, because that block
  is deployment topology no introspection can read off a database -- see the
  fixture README's "Regenerating" section.
- `support/` -- the harness. `docker/` is `process.rs` (driving the `docker`
  binary), `containers.rs`, `image_reference.rs`, `networks.rs`, and
  `polling.rs` (deadline-bounded,
  never a bare sleep), behind a `docker.rs` facade so every other file's
  `docker::` calls are unaffected by the split. `stack.rs` assembles postgres
  plus one connector mode; `impostor.rs` is a self-contained nginx standing
  in for "a real HTTP process that is not an NDC connector"; `compose.rs`
  publishes a fixture and assembles the real runtime and Data API over it;
  `fixtures.rs` and `requests.rs` are the publication fixture and the
  request/token helpers, deliberately not shared with
  `fabric-runtime-publication`'s own versions of each (see those files'
  module docs for why); `gate.rs` is the honest Docker-required-or-skip
  check every test calls first; `names.rs` and `images.rs` are naming and
  image pins.

## Gotchas

- The crate is `fabric-ndc-acceptance` (hyphen); the Rust identifier is
  `fabric_ndc_acceptance` (underscore).
- Every test in `tests/` calls `support::gate::docker_available_or_skip`
  first and returns immediately if it answers `false` -- so
  `cargo test -p fabric-ndc-acceptance` on a machine with no Docker daemon
  reports every test as passed while doing nothing (one stderr line per test
  naming why). That line is not something an ordinary run shows you: libtest
  captures a passing test's stderr and prints it only under `--nocapture` or
  when the test fails, so "visible" here means "visible if you go looking,"
  not "visible in the summary." Set `FABRIC_REQUIRE_CONNECTOR_ACCEPTANCE=1`
  to turn the skip into a hard, unmissable failure instead -- the
  `connector-acceptance` CI job always does, and that env var, not the
  stderr line, is the actual guarantee that a real connector was reached.
- The same variable also disables the fallback in
  `tests/support/docker/image_reference.rs`: `docker run` first pulls a
  pinned image absent locally by its digest, and only with the variable
  *unset* does a failed pull fall back to the bare tag. With it set, that
  failed pull is a hard failure naming the digest and the pull's own
  `stderr`, never a silent run of whatever the bare tag resolves to. On a
  sandboxed machine that cannot pull and only has the connector image loaded
  under a different (single-platform) digest than the pinned multi-arch index
  digest -- this repository's own situation on at least one development
  machine, see `tests/support/images.rs` -- the required mode therefore fails
  fast rather than passing. That is the mechanism working as intended, not a
  defect; CI's daemon pulls normally and is unaffected.
- Do not add a `[dependencies]` entry to make something "just easier to
  reach" from a future test. Everything this crate needs is a
  `[dev-dependencies]` edge, and that is load-bearing: a `[dev-dependencies]`
  edge only ever reaches this crate's own test binaries, never a production
  build, which is part of what keeps this crate out of both planes. This is
  also why `tests/support/fixtures.rs` builds publication documents from
  `serde_json::json!` literals rather than from `fabric_core` typed
  constructors, and why `tests/support/unsigned_reader.rs` implements its own
  minimal `TokenReader` rather than using `fabric_identity::TrustedIngressReader`:
  both would need a `fabric-core` dependency this crate does not have, and
  `scripts/check_architecture.py`'s dependency table does not list one.
- No containers or networks should outlive a test run. Every one is named
  under the `fabric-ndc-acc-` prefix (`tests/support/names.rs`), which is
  what lets `names::sweep_stale` find and remove a prior hard-killed run's
  leftovers at the start of the next one -- `Drop` does not run on `SIGKILL`.
  It skips anything carrying the current process's own pid segment, and
  anything younger than ten minutes, so two test binaries running at once
  cannot sweep away each other's still-live resources; only a resource that
  is both somebody else's and old enough to be certainly abandoned is
  removed.
