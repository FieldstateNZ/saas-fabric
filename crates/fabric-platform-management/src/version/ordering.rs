//! `SemVer` precedence.

use crate::Version;

/// The tuple a version is compared as.
///
/// Named because it is three nested shapes and the nesting *is* the rule:
/// core first, then whether a prerelease is present, then its identifiers.
/// An identifier compares as `(kind, digit_count, text)`: numeric
/// identifiers rank below alphanumeric ones, and among numeric identifiers a
/// longer one is a larger number, with equal-length ones compared as text.
/// That is only exact because `parse.rs` refuses a numeric identifier with a
/// leading zero — without that guarantee, two identifiers of equal length
/// could still mean different numbers padded to match. Comparing by digit
/// count also means there is no `u64` in this path, so unlike an arithmetic
/// parse there is no magnitude at which the comparison silently changes its
/// answer — the earlier version of this key parsed identifiers as `u64` and
/// fell back to string order on overflow, which is exactly that failure.
type Precedence<'a> = ((u64, u64, u64), u8, Vec<(u8, usize, &'a str)>);

impl Version {
    /// The precedence key.
    ///
    /// Three parts, in the order `SemVer` compares them: the version core, then
    /// whether a prerelease is present at all, then the prerelease
    /// identifiers. The middle one is the rule people forget — a version
    /// *with* a prerelease has lower precedence than the same core without
    /// one, so absence has to sort high.
    ///
    /// Within the identifiers, a numeric one ranks below an alphanumeric one,
    /// and a shorter list precedes a longer one that begins the same way.
    /// That falls out of comparing tuples in this order rather than needing
    /// to be written. See the `Precedence` doc comment above for how a
    /// numeric identifier itself compares.
    pub(crate) fn key(&self) -> Precedence<'_> {
        if self.pre.is_empty() {
            return (self.core, 1, Vec::new());
        }

        let parts = self
            .pre
            .iter()
            .map(|part| {
                if part.chars().all(|c| c.is_ascii_digit()) {
                    (0, part.len(), part.as_str())
                } else {
                    (1, 0, part.as_str())
                }
            })
            .collect();

        (self.core, 0, parts)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.text)
    }
}
