# fabric-ndc-acceptance

A test-only crate: the one place a test is allowed to compose the real
`fabric-runtime-publication` publisher, the real `fabric-tenant-runtime`
runtime, the real `fabric-data-api` Data API, and the real
`fabric-connector-ndc` adapter against a running NDC connector process. It
has no production code and ships nothing a caller could depend on.

This slice of issue #62 lands the crate boundary and the architecture-gate
amendment that admits it. The acceptance test itself -- publishing a fixture
through the real filesystem adapter, starting `ghcr.io/hasura/ndc-postgres`
as a subprocess, and driving the assembled router against it -- is a later
slice and does not exist here yet. `src/lib.rs` is documentation only.

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
all -- only `[dev-dependencies]`, naming the five internal crates the
eventual test composes (`fabric-runtime-publication`, `fabric-connector-ndc`,
`fabric-tenant-runtime`, `fabric-data-api`, `fabric-identity`,
`fabric-connector`) plus the transport and async crates that drive them
(`axum`, `http`, `tokio`, `tower`, `serde_json`). `publish = false`: this
crate is never meant to leave the workspace.

## Gotchas

- The crate is `fabric-ndc-acceptance` (hyphen); the Rust identifier is
  `fabric_ndc_acceptance` (underscore).
- `cargo test -p fabric-ndc-acceptance` currently builds an empty test
  binary -- there is no `tests/` directory yet. That is expected for this
  slice; the composed acceptance test is added in a later one.
- Do not add a `[dependencies]` entry to make something "just easier to
  reach" from a future test. Everything this crate needs is a
  `[dev-dependencies]` edge, and that is load-bearing: a `[dev-dependencies]`
  edge only ever reaches this crate's own test binaries, never a production
  build, which is part of what keeps this crate out of both planes.
