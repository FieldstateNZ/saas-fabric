//! The honest gate every test in this crate's `tests/` calls first.
//!
//! Not `#[ignore]` -- an ignored test is a test that passes by not running,
//! and the distinction between "ran and passed" and "did not run" must be a
//! property of the environment, not of anyone's memory (`m2-ndc/plan.md`
//! §4, lead decision 4).
//!
//! # [`REQUIRE_ENV`] also disables the pull-failure fallback
//!
//! This constant gates more than this file: `docker::run` (via
//! `docker::image_reference::resolve_runnable_reference`) checks whether a
//! pinned `image@sha256:...` reference is already present locally first,
//! and pulls it only when it is absent. Only *that* pull failing falls back
//! to the bare tag -- the situation this repository is in today for
//! `images::NDC_POSTGRES` on at least one development machine, whose daemon
//! cannot pull at all (see that constant's doc comment). With
//! [`REQUIRE_ENV`] set to `1`, that fallback is refused outright: a pull
//! failure becomes a failure naming the digest and the pull's own `stderr`,
//! never a silent run of whatever the bare tag happens to resolve to. CI
//! always sets it, so CI can never report success while running an image
//! other than the one it pinned -- the same "ran and passed" versus "did not
//! run" honesty this gate enforces for Docker's presence at all, applied to
//! *which* Docker image ran.

use crate::support::docker;

/// Set to `1`, this turns "no Docker daemon" from a skip into a panic, and
/// separately turns a pull failure on a missing pinned image into a failure
/// rather than a bare-tag fallback (see this module's doc comment above).
/// CI's `connector-acceptance` job sets it, so that job can never go green
/// without a real connector actually answering, nor by running an image
/// other than the one it pinned.
pub const REQUIRE_ENV: &str = "FABRIC_REQUIRE_CONNECTOR_ACCEPTANCE";

/// Whether `test_name` should proceed against a real Docker daemon.
///
/// - Docker answers `docker version`: returns `true`.
/// - It does not, and [`REQUIRE_ENV`] is unset: prints one line to stderr
///   naming why, then returns `false` -- the caller returns immediately, and
///   the test is reported as passed while doing nothing. "Reported" is a
///   claim about `cargo test`'s summary line, not about that stderr line
///   being seen: libtest captures a passing test's stdout/stderr and shows
///   it only under `--nocapture` (or when the test fails), so an ordinary
///   `cargo test -p fabric-ndc-acceptance` run prints none of this by
///   default. The actual guarantee that a real connector was reached is
///   [`REQUIRE_ENV`], not this stderr line -- see the next bullet.
/// - It does not, and [`REQUIRE_ENV`] is `1`: panics, naming the missing
///   prerequisite. A panicking test's output is never captured away, so
///   this failure is the one path here that is unconditionally seen.
///
/// # Panics
///
/// If [`REQUIRE_ENV`] is `1` and no Docker daemon answered.
#[must_use]
pub fn docker_available_or_skip(test_name: &str) -> bool {
    match docker::version() {
        Ok(()) => true,
        Err(error) => {
            assert!(
                std::env::var(REQUIRE_ENV).as_deref() != Ok("1"),
                "{test_name} requires a reachable Docker daemon because {REQUIRE_ENV}=1 is set, \
                 but `docker version` failed: {error}"
            );

            eprintln!(
                "{test_name}: skipped -- no Docker daemon available ({error}). Set \
                 {REQUIRE_ENV}=1 to require it (the connector-acceptance CI job always does)."
            );
            false
        }
    }
}
