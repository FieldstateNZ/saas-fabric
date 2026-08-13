//! The rule for turning a JSON number into a second of the validity window.

/// Converts a JSON number to a `NumericDate` in seconds.
///
/// # Agreeing with the other posture
///
/// Fractional values are rounded half away from zero, which is precisely what
/// `jsonwebtoken` 9's `numeric_type` deserialiser does (`value.round() as u64`).
/// [`ValidatingReader`](crate::ValidatingReader) delegates its window checks to
/// that library, so any other rule here would leave the two postures disagreeing
/// about the exact second a token expires.
///
/// # Dates outside `u64`
///
/// The cast saturates. That is Rust's defined behaviour for `as` on a float, not
/// an accident of the platform, and saturating happens to be the answer the
/// validity window wants at both ends:
///
/// - A date before the epoch clamps to `0`, which reads as "already expired" and
///   "already valid" — both true of an instant in 1969.
/// - A date beyond `u64::MAX` clamps there, which reads as "not expired yet" and
///   "not valid yet" — both true of an instant past representable time.
///
/// `jsonwebtoken` discards these instead, leaving the claim to constrain
/// nothing; it only avoids honouring a stale `exp` that way because it
/// separately requires that claim to be present and parseable. Clamping is the
/// stricter rule and needs no such backstop.
///
/// # No NaN case
///
/// `serde_json` cannot hand us one. The JSON grammar has no NaN or infinity
/// literal, `1e400` is a parse error rather than an infinity, and
/// `Number::from_f64` refuses both — so a non-finite date never reaches here. A
/// test pins that, which is worth more than a branch guessing at what such a
/// date would have meant.
pub(super) fn to_numeric_date(value: f64) -> u64 {
    // Truncation and sign loss are the point; see "Dates outside `u64`".
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let seconds = value.round() as u64;

    seconds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_float_keeps_its_second() {
        assert_eq!(to_numeric_date(1_000.0), 1_000);
    }

    #[test]
    fn rounds_half_away_from_zero_exactly_as_the_other_posture_does() {
        assert_eq!(to_numeric_date(1_000.4), 1_000);
        assert_eq!(to_numeric_date(1_000.5), 1_001);
        assert_eq!(to_numeric_date(1_000.6), 1_001);
    }

    #[test]
    fn a_date_before_the_epoch_clamps_to_zero() {
        assert_eq!(to_numeric_date(-1.0), 0);
        assert_eq!(to_numeric_date(-1.5), 0);
        assert_eq!(to_numeric_date(f64::MIN), 0);
    }

    #[test]
    fn a_date_beyond_the_representable_range_clamps_to_the_maximum() {
        assert_eq!(to_numeric_date(1e30), u64::MAX);
        assert_eq!(to_numeric_date(f64::MAX), u64::MAX);
    }

    #[test]
    fn nothing_panics_on_the_values_that_have_no_second() {
        // Unreachable through a token, but the saturating cast is total, so
        // these are answers rather than aborts. Documented because the reason
        // they cannot arrive lives in `claims.rs`, not here.
        assert_eq!(to_numeric_date(f64::INFINITY), u64::MAX);
        assert_eq!(to_numeric_date(f64::NEG_INFINITY), 0);
        assert_eq!(to_numeric_date(f64::NAN), 0);
    }
}
