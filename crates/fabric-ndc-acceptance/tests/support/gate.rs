//! The honest gate every test in this crate's `tests/` calls first.
//!
//! Not `#[ignore]` -- an ignored test is a test that passes by not running,
//! and the distinction between "ran and passed" and "did not run" must be a
//! property of the environment, not of anyone's memory (`m2-ndc/plan.md`
//! §4, lead decision 4).

use crate::support::docker;

/// Set to `1`, this turns "no Docker daemon" from a skip into a panic. CI's
/// `connector-acceptance` job sets it, so that job can never go green
/// without a real connector actually answering.
pub const REQUIRE_ENV: &str = "FABRIC_REQUIRE_CONNECTOR_ACCEPTANCE";

/// Whether `test_name` should proceed against a real Docker daemon.
///
/// - Docker answers `docker version`: returns `true`.
/// - It does not, and [`REQUIRE_ENV`] is unset: prints one line to stderr
///   naming why, then returns `false` -- the caller returns immediately,
///   and the test is reported as passed-but-did-nothing, visibly, rather
///   than silently.
/// - It does not, and [`REQUIRE_ENV`] is `1`: panics, naming the missing
///   prerequisite.
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
