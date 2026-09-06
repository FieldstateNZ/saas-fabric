//! Parses Docker's `{{.CreatedAt}}` timestamp format into Unix epoch
//! seconds.
//!
//! `docker ps`/`docker network ls --format '{{.CreatedAt}}'` emit exactly
//! Go's `time.Time.String()` layout (for example
//! `"2024-01-15 10:23:45.123456789 +0000 UTC"`) -- a fixed format documented
//! by the Go standard library, not a local host convention, so nothing here
//! depends on this machine's locale or date settings. Split out of
//! `names.rs` (`docs/architecture/file-size-policy.md`'s "one concept per
//! file" convention, which reviewers hold test support to as well): parsing
//! a fixed third-party timestamp layout is a self-contained concept, unlike
//! the naming scheme and sweep policy that consume its result.
//!
//! Hand-rolled rather than a date/time dependency: this workspace adds none
//! for a single freshness comparison, and the layout this parses is fixed
//! by Go's standard library, not by anything a date crate would need to
//! interpret per host.

/// Parses the fixed layout `docker ps`/`docker network ls` use for
/// `{{.CreatedAt}}` -- Go's `time.Time.String()` -- into Unix epoch seconds.
///
/// Returns `None` on anything that does not match closely enough to trust;
/// see [`crate::support::names::is_stale`] for what happens then.
pub(super) fn parse_docker_created_at(text: &str) -> Option<i64> {
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
    use super::{days_from_civil, parse_docker_created_at};

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
}
