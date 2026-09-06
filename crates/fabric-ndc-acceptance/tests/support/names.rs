//! Container and network naming, and the stale-resource sweep.
//!
//! `Drop` does not run on `SIGKILL`, so a hard-killed test run can leave
//! containers and the network behind. The prefix every name in this module
//! carries is what lets [`sweep_stale`] find them again on the next run.
//! Parsing Docker's own timestamp format is a separate concept, split out
//! to [`super::go_timestamp`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::support::docker;
use crate::support::go_timestamp::parse_docker_created_at;

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
    /// name is worse than a slightly longer one. The pid segment is also
    /// what lets [`sweep_stale`] tell this process's own, still-live
    /// resources apart from a different, hard-killed run's leftovers.
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

/// How many seconds old a leftover must be before [`sweep_stale`] removes
/// it.
///
/// Longer than the twenty-minute `timeout-minutes` the `connector-acceptance`
/// CI job runs under (`.github/workflows/ci.yml`), so no CI run can still be
/// in progress when its own resources become sweep-eligible -- that is the
/// actual guarantee this constant gives, and it is why the value is chosen
/// against that job's timeout rather than picked for convenience. It does
/// not protect every concurrent run unconditionally: see [`sweep_stale`]'s
/// doc for what a run longer than this, outside CI, is still exposed to.
const STALE_AFTER_SECS: i64 = 30 * 60;

/// Removes every container and network under [`PREFIX`] left over from a
/// prior, hard-killed run. Runs at most once per test binary process: called
/// from every [`crate::support::stack::Stack::up`], guarded so the first
/// caller does it before anything in this run exists, and every later
/// caller is a no-op.
///
/// Two concurrent test binaries -- two `cargo test` invocations, or two
/// tests in the same binary racing on different threads -- would otherwise
/// sweep each other's still-live resources: process B's sweep, running
/// before process A has finished with the network or containers it just
/// started, would remove them out from under A's still-running test. The
/// pid check in [`is_stale`] only ever protects *this* process's own
/// resources from itself; it cannot distinguish a genuinely stale leftover
/// from a different, concurrently running process's resources, because both
/// carry a pid that is not this one. [`STALE_AFTER_SECS`] is what actually
/// stands in for that: raised to thirty minutes specifically so that no CI
/// run (capped at twenty) can still be alive when a concurrent sweep would
/// consider its resources fair game.
///
/// **This does not close the gap for every shape of concurrent run.** A
/// local, interactive run of this suite left going for longer than
/// [`STALE_AFTER_SECS`] while a second invocation starts could still have
/// its containers and network removed by that second run's sweep -- the pid
/// check does not save it, and thirty minutes have genuinely passed. That is
/// accepted: CI, the one unattended environment this sweep exists to keep
/// clean of hard-kill leftovers, is covered by construction against its own
/// timeout; a developer running the suite by hand for more than half an hour
/// while starting a second run concurrently is a shape rare and visible
/// enough -- the affected run's containers simply vanish and it fails
/// loudly -- to accept rather than design further guards against.
///
/// Best-effort: a container or network that fails to remove is left for the
/// next sweep rather than failing the test that happened to run first.
pub fn sweep_stale() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let this_pid = std::process::id().to_string();
        let now_epoch = unix_epoch_seconds_now();

        for (name, created_at) in docker::container_summaries_with_prefix(PREFIX).unwrap_or_default() {
            if is_stale(&name, &created_at, &this_pid, now_epoch) {
                let _ = docker::rm_by_name(&name);
            }
        }
        for (name, created_at) in docker::network_summaries_with_prefix(PREFIX).unwrap_or_default() {
            if is_stale(&name, &created_at, &this_pid, now_epoch) {
                let _ = docker::network_rm(&name);
            }
        }
    });
}

/// Whether `name` (with Docker's own `created_at` reading for it) belongs to
/// a different, hard-killed run old enough to remove safely.
///
/// Either guard failing to clear is enough to answer `false`: a name
/// carrying `this_pid`'s own segment can only have been created by this same
/// process (see [`RunId::new`]), which never needs sweeping from under
/// itself; and a resource younger than [`STALE_AFTER_SECS`] may belong to a
/// concurrent run that started moments ago and has not finished with it yet.
/// An unparseable `created_at` also answers `false` -- see
/// [`parse_docker_created_at`] -- rather than guess at an age it could not
/// read.
fn is_stale(name: &str, created_at: &str, this_pid: &str, now_epoch: i64) -> bool {
    if pid_segment(name) == Some(this_pid) {
        return false;
    }

    let Some(created_epoch) = parse_docker_created_at(created_at) else {
        return false;
    };

    now_epoch.saturating_sub(created_epoch) >= STALE_AFTER_SECS
}

/// The pid segment [`RunId::new`] embedded in `name` -- the token
/// immediately after [`PREFIX`] -- or `None` if `name` does not start with
/// [`PREFIX`] at all.
fn pid_segment(name: &str) -> Option<&str> {
    name.strip_prefix(PREFIX)?.split('-').next()
}

/// The current time as Unix epoch seconds, or `i64::MIN` if it cannot be
/// read. `i64::MIN` makes every subsequent [`is_stale`] check answer
/// `false` (a huge negative age), which is the safe direction to fail in --
/// leaving a leftover for the next sweep costs nothing a hard-killed run's
/// resources were not already costing; removing a live one mid-test would
/// not be recoverable.
fn unix_epoch_seconds_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since_epoch| i64::try_from(since_epoch.as_secs()).ok())
        .unwrap_or(i64::MIN)
}

#[cfg(test)]
mod tests {
    use super::{is_stale, parse_docker_created_at, pid_segment, PREFIX};

    #[test]
    fn the_pid_segment_is_the_token_right_after_the_prefix() {
        assert_eq!(pid_segment("fabric-ndc-acc-4242-999-0-postgres"), Some("4242"));
    }

    #[test]
    fn a_name_missing_the_prefix_has_no_pid_segment() {
        assert_eq!(pid_segment("some-other-container"), None);
    }

    #[test]
    fn a_name_carrying_this_process_own_pid_is_never_stale() {
        // Old enough by age alone, but it is "our" pid -- never swept.
        let ancient = "2000-01-01 00:00:00 +0000 UTC";
        assert!(!is_stale(
            "fabric-ndc-acc-777-1-0-postgres",
            ancient,
            "777",
            4_102_444_800
        ));
    }

    #[test]
    fn a_resource_younger_than_thirty_minutes_is_never_stale_regardless_of_pid() {
        let created = parse_docker_created_at("2024-01-15 10:23:45 +0000 UTC").unwrap();
        let twenty_nine_minutes_later = created + 29 * 60;
        assert!(!is_stale(
            &format!("{PREFIX}999-1-0-postgres"),
            "2024-01-15 10:23:45 +0000 UTC",
            "777",
            twenty_nine_minutes_later
        ));
    }

    #[test]
    fn a_different_pids_resource_older_than_thirty_minutes_is_stale() {
        let created = parse_docker_created_at("2024-01-15 10:23:45 +0000 UTC").unwrap();
        let thirty_one_minutes_later = created + 31 * 60;
        assert!(is_stale(
            &format!("{PREFIX}999-1-0-postgres"),
            "2024-01-15 10:23:45 +0000 UTC",
            "777",
            thirty_one_minutes_later
        ));
    }
}
