//! Polling to a deadline, so no sleep alone ever decides a test's outcome.
//!
//! The "polling" concept the parent `docker.rs` module doc names. Knows
//! nothing about `docker` itself -- callers pass whatever readiness check
//! they need (`pg_isready`, a health-check binary, a log line appearing
//! twice), which is what lets [`poll_until`] serve postgres, the connector,
//! and nginx alike.

use std::time::{Duration, Instant};

/// Calls `attempt` in a loop until it returns `true` or `deadline` has
/// elapsed since this call started, sleeping briefly in between. The
/// deadline bounds how long polling continues; it is never itself the
/// signal that the condition holds.
#[must_use]
pub fn poll_until(deadline: Duration, mut attempt: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if attempt() {
            return true;
        }
        if start.elapsed() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
