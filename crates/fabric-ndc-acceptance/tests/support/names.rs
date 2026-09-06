//! Container and network naming, and the stale-resource sweep.
//!
//! `Drop` does not run on `SIGKILL`, so a hard-killed test run can leave
//! containers and the network behind. The prefix every name in this module
//! carries is what lets [`sweep_stale`] find them again on the next run.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::support::docker;

/// Every name this harness gives Docker starts with this.
pub const PREFIX: &str = "fabric-ndc-acc-";

/// One test run's naming scope: a process-unique base every container name
/// and the run's one network are built from.
pub struct RunId {
    base: String,
}

impl RunId {
    /// A fresh, process-unique base: pid, a nanosecond timestamp, and a
    /// counter -- the same three-part scheme `fabric-runtime-publication`'s
    /// `TempDir` uses, for the same reason: several tests in this binary can
    /// call this within the same nanosecond, and two of them landing on one
    /// name is worse than a slightly longer one.
    #[must_use]
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        // The clock is never before the epoch.
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let base = format!(
            "{PREFIX}{}-{nanos}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        Self { base }
    }

    /// The name for one role's container within this run (`postgres`,
    /// `connector`, `nginx`).
    #[must_use]
    pub fn container_name(&self, role: &str) -> String {
        format!("{}-{role}", self.base)
    }

    /// This run's one Docker network.
    #[must_use]
    pub fn network_name(&self) -> String {
        format!("{}-net", self.base)
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

/// Removes every container and network under [`PREFIX`] left over from a
/// prior run. Runs at most once per test binary process: called from every
/// [`crate::support::stack::Stack::up`], guarded so the first caller does it
/// before anything in this run exists, and every later caller is a no-op.
///
/// Best-effort: a container or network that fails to remove is left for the
/// next sweep rather than failing the test that happened to run first.
pub fn sweep_stale() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        for name in docker::container_names_with_prefix(PREFIX).unwrap_or_default() {
            let _ = docker::rm_by_name(&name);
        }
        for name in docker::network_names_with_prefix(PREFIX).unwrap_or_default() {
            let _ = docker::network_rm(&name);
        }
    });
}
