//! How long a minted installation token is treated as usable.

use std::time::Duration;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Subtracted from the host's stated expiry.
///
/// Covers the round trip that presents the token and any clock difference
/// between this process and the host. Five minutes is generous against a token
/// issued for an hour and costs one extra mint per hour.
const MARGIN: Duration = Duration::from_mins(5);

/// The shortest a token is ever cached for.
///
/// Without a floor, a token already near its expiry — or one whose stated
/// expiry is behind this machine's clock — would be re-minted on every single
/// request. A short cache plus the `401` retry in `operations` is a better
/// answer than a mint per call.
const MINIMUM: Duration = Duration::from_secs(30);

/// The longest, regardless of what the host claims.
///
/// A host reporting an implausible expiry — a clock a year out, a field that
/// stopped meaning what it meant — must not cause the platform to hold one
/// token indefinitely. Bounded well inside GitHub's documented hour.
const MAXIMUM: Duration = Duration::from_mins(55);

/// Used when the stated expiry cannot be read at all.
///
/// Deliberately short rather than zero. Zero would mint per request; a longer
/// value would extend a guess. Five minutes plus the `401` retry means an
/// unreadable expiry costs a few extra mints an hour and nothing else.
const UNREADABLE: Duration = Duration::from_mins(5);

/// Works out how long a token minted now may be cached.
///
/// # Why the result is a duration and not a deadline
///
/// The caller turns it into a **monotonic** deadline. The host states its
/// expiry in wall-clock time, so the *remaining lifetime* has to be computed
/// against wall-clock now — but a deadline held in wall-clock time would move
/// under an NTP step, either expiring every cached token at once or extending
/// one past its real life. Computing the difference here and measuring it
/// monotonically there takes the accurate part of each.
pub(super) fn usable_for(expires_at: &str, now_unix: u64) -> Duration {
    let Ok(expiry) = OffsetDateTime::parse(expires_at, &Rfc3339) else {
        return UNREADABLE;
    };

    let remaining = expiry
        .unix_timestamp()
        .saturating_sub(i64::try_from(now_unix).unwrap_or(i64::MAX));

    let Ok(remaining) = u64::try_from(remaining) else {
        // Already expired by this machine's clock. The floor applies: the
        // token may still work, and finding out costs one request.
        return MINIMUM;
    };

    Duration::from_secs(remaining)
        .saturating_sub(MARGIN)
        .clamp(MINIMUM, MAXIMUM)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-08-28T09:00:00Z, as unix seconds.
    const NINE_AM: u64 = 1_787_907_600;

    #[test]
    fn an_hour_long_token_is_cached_for_the_hour_less_the_margin() {
        let usable = usable_for("2026-08-28T10:00:00Z", NINE_AM);

        assert_eq!(usable, Duration::from_mins(55));
    }

    #[test]
    fn a_shorter_lifetime_than_github_documents_is_honoured() {
        // The defect this closes: a fixed fifty-minute cache would have held a
        // ten-minute token for forty minutes after it stopped working.
        let usable = usable_for("2026-08-28T09:10:00Z", NINE_AM);

        assert_eq!(usable, Duration::from_mins(5));
    }

    #[test]
    fn an_implausibly_distant_expiry_is_capped() {
        let usable = usable_for("2027-08-28T09:00:00Z", NINE_AM);

        assert_eq!(usable, MAXIMUM);
    }

    #[test]
    fn an_expiry_already_past_falls_to_the_floor_rather_than_zero() {
        // Zero would mint on every request. The floor plus the `401` retry is
        // the cheaper way to find out whether the token still works.
        assert_eq!(usable_for("2026-08-28T08:00:00Z", NINE_AM), MINIMUM);
    }

    #[test]
    fn an_expiry_inside_the_margin_falls_to_the_floor() {
        assert_eq!(usable_for("2026-08-28T09:01:00Z", NINE_AM), MINIMUM);
    }

    #[test]
    fn an_unreadable_expiry_is_short_rather_than_absent_or_assumed() {
        for stated in ["", "soon", "2026-08-28 09:00:00", "not-a-timestamp"] {
            assert_eq!(usable_for(stated, NINE_AM), UNREADABLE, "{stated}");
        }
    }

    #[test]
    fn an_offset_timestamp_is_read_as_the_instant_it_names() {
        // RFC 3339 permits an offset. `10:00:00+01:00` is 09:00:00Z, which is
        // *now* — so this must fall to the floor rather than being read as an
        // hour away.
        assert_eq!(usable_for("2026-08-28T10:00:00+01:00", NINE_AM), MINIMUM);
    }
}
