//! `SemVer` precedence.

use crate::Version;

/// The tuple a version is compared as.
///
/// Named because it is three nested shapes and the nesting *is* the rule:
/// core first, then whether a prerelease is present, then its identifiers.
type Precedence<'a> = ((u64, u64, u64), u8, Vec<(u8, u64, &'a str)>);

impl Version {
    /// The precedence key.
    ///
    /// Three parts, in the order `SemVer` compares them: the version core, then
    /// whether a prerelease is present at all, then the prerelease
    /// identifiers. The middle one is the rule people forget — a version
    /// *with* a prerelease has lower precedence than the same core without
    /// one, so absence has to sort high.
    ///
    /// Within the identifiers, a numeric one compares numerically and ranks
    /// below an alphanumeric one, and a shorter list precedes a longer one
    /// that begins the same way. That falls out of comparing tuples in this
    /// order rather than needing to be written.
    pub(crate) fn key(&self) -> Precedence<'_> {
        if self.pre.is_empty() {
            return (self.core, 1, Vec::new());
        }

        let parts = self
            .pre
            .iter()
            .map(|part| match part.parse::<u64>() {
                Ok(number) => (0, number, ""),
                Err(_) => (1, 0, part.as_str()),
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
