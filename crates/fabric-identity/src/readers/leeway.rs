//! The clock-skew allowance both postures apply to the validity window.

/// A checked clock-skew allowance, in seconds.
///
/// # Why this is a type rather than an integer
///
/// Leeway exists to widen the token's validity window symmetrically, so that a
/// host whose clock has drifted still accepts tokens that are genuinely current.
/// Two kinds of value defeat that purpose instead of tuning it:
///
/// - A **negative** allowance *narrows* the window, and past `-exp` inverts it,
///   so tokens well inside their own validity period start being refused.
/// - An **enormous** allowance swallows the window whole. At `i64::MAX` neither
///   `exp` nor `nbf` constrains anything, while every call site still reads as
///   though both are checked.
///
/// Both were reachable. [`TrustedIngressReader`](crate::TrustedIngressReader)
/// took a bare `i64` and validated nothing, while
/// [`ValidatingReader`](crate::ValidatingReader) took a `u64` — two spellings of
/// one concept, neither checked, in a pair of readers whose whole contract is to
/// agree. Making the range a property of the type means neither reader has to
/// remember to check, and they cannot drift apart again, because they now name
/// the same type.
///
/// # Why the ceiling is an hour
///
/// Skew allowances in the wild are tens of seconds, and the default here is one
/// minute; NTP holds hosts far closer than that. An hour is therefore already
/// two orders of magnitude of headroom. A deployment that believes it needs more
/// has a broken clock to repair, and widening the window further would hide that
/// symptom rather than fix its cause.
///
/// # Reading one from configuration
///
/// Deserialising goes through [`Self::try_new`] via `#[serde(try_from)]`, so a
/// configuration file cannot produce an unchecked value the constructor would
/// have refused. Combined with [`Default`] that makes it a one-line field:
///
/// ```
/// use fabric_identity::LeewaySeconds;
///
/// #[derive(serde::Deserialize)]
/// struct Example {
///     #[serde(default)]
///     leeway_seconds: LeewaySeconds,
/// }
///
/// let configured: Example = serde_json::from_str(r#"{"leeway_seconds":120}"#)?;
/// assert_eq!(configured.leeway_seconds.seconds(), 120);
///
/// // Omitted entirely, the default applies.
/// let defaulted: Example = serde_json::from_str("{}")?;
/// assert_eq!(defaulted.leeway_seconds, LeewaySeconds::DEFAULT);
///
/// // Out of range fails at parse time, not at first request.
/// assert!(serde_json::from_str::<Example>(r#"{"leeway_seconds":86400}"#).is_err());
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
#[serde(try_from = "u64")]
pub struct LeewaySeconds(u64);

impl LeewaySeconds {
    /// The allowance applied unless a deployment overrides it: one minute.
    pub const DEFAULT: Self = Self(60);

    /// The largest allowance a deployment may configure, in seconds.
    pub const MAX_SECONDS: u64 = 3_600;

    /// Builds an allowance from a number of seconds.
    ///
    /// # Errors
    ///
    /// Returns a message naming the permitted range when `seconds` exceeds
    /// [`Self::MAX_SECONDS`]. There is deliberately no check for a negative
    /// value: the parameter is unsigned, so one cannot be spelled.
    pub fn try_new(seconds: u64) -> Result<Self, String> {
        if seconds > Self::MAX_SECONDS {
            return Err(format!(
                "identity leeway must be between 0 and {} seconds, but was {seconds}",
                Self::MAX_SECONDS
            ));
        }

        Ok(Self(seconds))
    }

    /// The allowance in seconds.
    #[must_use]
    pub const fn seconds(self) -> u64 {
        self.0
    }
}

impl Default for LeewaySeconds {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TryFrom<u64> for LeewaySeconds {
    type Error = String;

    /// The single checked entry point, so `serde` and hand-written callers
    /// cannot disagree about what is in range.
    fn try_from(seconds: u64) -> Result<Self, Self::Error> {
        Self::try_new(seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_allowance_is_one_minute() {
        assert_eq!(LeewaySeconds::default().seconds(), 60);
    }

    #[test]
    fn accepts_an_ordinary_skew_allowance() {
        assert_eq!(LeewaySeconds::try_new(120).unwrap().seconds(), 120);
    }

    #[test]
    fn accepts_no_allowance_at_all() {
        // Zero is a legitimate choice for a deployment with a disciplined
        // clock; it is only the *upper* end that can neutralise the window.
        assert_eq!(LeewaySeconds::try_new(0).unwrap().seconds(), 0);
    }

    #[test]
    fn accepts_the_ceiling_itself() {
        assert!(LeewaySeconds::try_new(LeewaySeconds::MAX_SECONDS).is_ok());
    }

    #[test]
    fn rejects_one_second_beyond_the_ceiling() {
        assert!(LeewaySeconds::try_new(LeewaySeconds::MAX_SECONDS + 1).is_err());
    }

    #[test]
    fn rejects_an_allowance_that_would_disable_the_validity_window() {
        // The value the adversarial review reached for: with this accepted,
        // `exp` and `nbf` would both stop constraining anything.
        assert!(LeewaySeconds::try_new(u64::MAX).is_err());
    }

    #[test]
    fn the_rejection_message_names_the_permitted_range() {
        let message = LeewaySeconds::try_new(u64::MAX).unwrap_err();

        assert!(message.contains("3600"), "unhelpful message: {message}");
    }

    #[test]
    fn deserialises_an_in_range_allowance_from_configuration() {
        let leeway: LeewaySeconds = serde_json::from_str("120").unwrap();

        assert_eq!(leeway.seconds(), 120);
    }

    #[test]
    fn an_out_of_range_allowance_fails_to_deserialise_rather_than_being_clamped() {
        // The point of `try_from`: config that would neutralise the validity
        // window stops the process at startup instead of taking effect.
        let error = serde_json::from_str::<LeewaySeconds>("86400").unwrap_err();

        assert!(error.to_string().contains("3600"), "unhelpful message: {error}");
    }

    #[test]
    fn a_negative_allowance_cannot_even_be_spelled_in_configuration() {
        // The inner type is unsigned, so this fails in `serde` before the
        // range check is reached. Recorded because a negative leeway narrows
        // the window it exists to widen.
        assert!(serde_json::from_str::<LeewaySeconds>("-1").is_err());
    }
}
