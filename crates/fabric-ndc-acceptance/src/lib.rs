//! The one place a test may compose the real publisher, the real runtime,
//! the real Data API, and the real NDC adapter against a running connector.
//!
//! Issue #62 needs an acceptance test that proves the whole chain end to
//! end: `fabric-runtime-publication` publishes a fixture, the real
//! `fabric_tenant_runtime::build_runtime` reads it, the real
//! `fabric_data_api::build_data_api` serves it, and the real
//! `fabric-connector-ndc` adapter executes against an actual
//! `ghcr.io/hasura/ndc-postgres` process rather than a fake. No existing
//! crate can host that composition without failing an architecture check --
//! see "Why this crate exists" below -- so this one exists to hold it. The
//! composed test itself is
//! `tests/published_state_reaches_a_real_connector.rs`; the container
//! harness it depends on lives beside it under `tests/support/`, and
//! `tests/the_stack_comes_up.rs` is that harness's own proof that it comes
//! up and answers. `docs/verification.md`'s "Connector acceptance (issue
//! #62)" section is the fuller account of what each test proves.
//!
//! # Why this crate exists, in neither plane
//!
//! Three checks in `scripts/check_architecture.py` interact, and between
//! them they block every other placement for this test:
//!
//! 1. `check_ndc_containment`'s source scan (the function starts at line
//!    323, the scanning loop at lines 328-349) reads every `.rs` file in a
//!    crate, `tests/` included -- deliberately, because an integration test
//!    is compiled Rust that can name whatever it likes. Only
//!    `fabric-connector-ndc` itself is exempt outright; `fabric-api` (the
//!    host) may additionally name `NdcConnectorConfig` and
//!    `build_ndc_connector` (`NDC_NAMES_THE_HOST_MAY_USE`, lines 109-114)
//!    and nothing else. A test naming the adapter's request or response
//!    types from `fabric-runtime-publication/tests/` would fail here.
//! 2. The same function's dependency-edge loop (lines 352-364) fails any
//!    crate other than the NDC crate or the host that declares a dependency
//!    on `fabric-connector-ndc` -- and `Graph.direct_dependencies` is
//!    dev-inclusive, so a `[dev-dependencies]` edge counts exactly like a
//!    production one. `fabric-runtime-publication` dev-depending on the NDC
//!    crate to drive this test would fail here.
//! 3. `check_runtime_plane_cannot_reach_the_publisher` (lines 765-809) walks
//!    every `RUNTIME_PLANE` crate's dependency closure, dev tables
//!    included, and fails if it reaches `fabric-runtime-publication`.
//!    `RUNTIME_PLANE` contains `fabric-connector-ndc`, `fabric-data-api` and
//!    `fabric-api`, so none of the three can host this test either: any of
//!    them dev-depending on the publisher would make that crate a second,
//!    test-shaped path to the one writer ADR 0018's fence exists to keep
//!    alone.
//!
//! Nowhere existing can dev-depend on both `fabric-runtime-publication` and
//! `fabric-connector-ndc` without tripping one of the three gates above.
//! `fabric-ndc-acceptance` is a new crate in neither plane -- the same
//! footing as `fabric-core` and `fabric-runtime-publication` itself -- so
//! nothing in `RUNTIME_PLANE` or `CONTROL_PLANE` depends on it, and its
//! dependency closure reaches nothing either plane depends on. That keeps
//! the publisher fence exactly as narrow as ADR 0018 left it:
//! `scripts/check_architecture.py` admits this crate by name, in both loops
//! of `check_ndc_containment`, rather than by widening either gate.
//!
//! # No production code
//!
//! This crate has no `[dependencies]`. Everything it names --
//! `fabric-runtime-publication`, `fabric-connector-ndc`,
//! `fabric-tenant-runtime`, `fabric-data-api`, `fabric-identity`,
//! `fabric-connector`, the transport crates that drive them (`axum`,
//! `http`, `tokio`, `tower`, `serde_json`), and `base64` (decodes this
//! crate's own unsigned test tokens; see `tests/support/unsigned_reader.rs`) --
//! is a `[dev-dependencies]` edge, reachable only from this crate's own test
//! binaries and never from a production build. Deliberately absent:
//! `fabric-core`, which `scripts/check_architecture.py`'s dependency table
//! does not list for this crate -- see `tests/support/fixtures.rs` and
//! `tests/support/unsigned_reader.rs` for the two places that shapes what
//! the test harness can and cannot build directly. This file carries no
//! code, only the documentation a reviewer needs before reading the test
//! itself.
