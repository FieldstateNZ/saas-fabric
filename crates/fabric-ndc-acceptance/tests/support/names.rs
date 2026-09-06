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
/// it. Shorter than this and a concurrent run's own resources -- freshly
/// created, moments ago, by a different process -- would look exactly like
/// a prior hard-killed run's leftovers; the pid check alone cannot catch
/// that case, because it is by definition a different pid.
const STALE_AFTER_SECS: i64 = 10 * 60;

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
/// started, would remove them out from under A's still-running test. Two
/// independent guards close that, either of which is enough to leave a
/// resource alone -- see [`is_stale`]: a name carrying a *different*
/// process's pid, and old enough ([`STALE_AFTER_SECS`]) that it cannot be
/// some other run's work still in progress.
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

/// Parses the fixed layout `docker ps`/`docker network ls` use for
/// `{{.CreatedAt}}` -- Go's `time.Time.String()`, e.g.
/// `"2024-01-15 10:23:45.123456789 +0000 UTC"` -- into Unix epoch seconds.
///
/// Hand-rolled rather than a date/time dependency: this workspace adds none
/// for a ten-minute freshness check, and the layout is fixed by Go's
/// standard library, not by the local host's date conventions. Returns
/// `None` on anything that does not match closely enough to trust; see
/// [`is_stale`] for what happens then.
fn parse_docker_created_at(text: &str) -> Option<i64> {
    let mut fields = text.split_whitespace();
    let date = fields.next()?;
    let time = fields.next()?;
    let offset = fields.next()?;

    let mut date_parts = date.splitn(3, '-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;

    let time_without_fraction = time.split('.').next()?;
    let mut time_parts = time_without_fraction.splitn(3, ':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let second: i64 = time_parts.next()?.parse().ok()?;

    let sign: i64 = match offset.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let offset_digits = offset.get(1..)?;
    if offset_digits.len() != 4 {
        return None;
    }
    let offset_hours: i64 = offset_digits.get(0..2)?.parse().ok()?;
    let offset_minutes: i64 = offset_digits.get(2..4)?.parse().ok()?;
    let offset_seconds = sign * (offset_hours * 3600 + offset_minutes * 60);

    let local_seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    Some(local_seconds - offset_seconds)
}

/// Days since the Unix epoch for a proleptic-Gregorian civil date. Howard
/// Hinnant's public-domain `days_from_civil` algorithm
/// (<https://howardhinnant.github.io/date_algorithms.html>).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = if shifted_year >= 0 {
        shifted_year
    } else {
        shifted_year - 399
    } / 400;
    let year_of_era = shifted_year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::{days_from_civil, is_stale, parse_docker_created_at, pid_segment, PREFIX};

    #[test]
    fn the_unix_epoch_itself_is_day_zero() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn a_known_recent_date_matches_its_known_day_count() {
        // 2024-01-15 is 19737 days after 1970-01-01.
        assert_eq!(days_from_civil(2024, 1, 15), 19_737);
    }

    #[test]
    fn a_utc_created_at_round_trips_through_the_parser() {
        let epoch = parse_docker_created_at("2024-01-15 10:23:45.123456789 +0000 UTC").unwrap();
        assert_eq!(epoch, 19_737 * 86_400 + 10 * 3600 + 23 * 60 + 45);
    }

    #[test]
    fn a_negative_offset_shifts_the_epoch_forward() {
        let with_offset = parse_docker_created_at("2024-01-15 10:23:45 -0700 MST").unwrap();
        let utc = parse_docker_created_at("2024-01-15 10:23:45 +0000 UTC").unwrap();
        assert_eq!(with_offset - utc, 7 * 3600);
    }

    #[test]
    fn an_unrecognised_shape_parses_to_nothing() {
        assert_eq!(parse_docker_created_at("not a timestamp"), None);
    }

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
    fn a_resource_younger_than_ten_minutes_is_never_stale_regardless_of_pid() {
        let created = parse_docker_created_at("2024-01-15 10:23:45 +0000 UTC").unwrap();
        let five_minutes_later = created + 5 * 60;
        assert!(!is_stale(
            &format!("{PREFIX}999-1-0-postgres"),
            "2024-01-15 10:23:45 +0000 UTC",
            "777",
            five_minutes_later
        ));
    }

    #[test]
    fn a_different_pids_resource_older_than_ten_minutes_is_stale() {
        let created = parse_docker_created_at("2024-01-15 10:23:45 +0000 UTC").unwrap();
        let eleven_minutes_later = created + 11 * 60;
        assert!(is_stale(
            &format!("{PREFIX}999-1-0-postgres"),
            "2024-01-15 10:23:45 +0000 UTC",
            "777",
            eleven_minutes_later
        ));
    }
}
