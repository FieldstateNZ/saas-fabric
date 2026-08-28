//! The clock seam, so time-dependent behaviour stays testable.

use std::sync::Arc;
use std::time::Instant;

/// Supplies the current time.
///
/// The connection pool manager evicts idle pools and rotates credentials on a
/// schedule. Testing that behaviour against the real clock would mean sleeping
/// for the idle timeout, so time is injected instead and tests advance it
/// instantly.
///
/// Monotonic [`Instant`] is used for durations rather than wall-clock time
/// because pool eviction must not be affected by an NTP step or a daylight
/// saving transition. Wall-clock time is exposed separately, and only for
/// stamping records that a human will read.
pub trait Clock: Send + Sync {
    /// Returns a monotonically non-decreasing instant, for measuring elapsed
    /// time.
    fn now(&self) -> Instant;

    /// Returns wall-clock time as seconds since the Unix epoch, for timestamps
    /// that leave the process.
    fn now_unix_seconds(&self) -> u64;
}

/// The production [`Clock`], backed by the operating system.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl SystemClock {
    /// Creates a system clock.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Creates a system clock behind an [`Arc`], ready to hand to services.
    #[must_use]
    pub fn shared() -> Arc<dyn Clock> {
        Arc::new(Self)
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs())
    }
}
