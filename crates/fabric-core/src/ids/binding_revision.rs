//! The monotonically increasing revision stamped on every runtime binding.

use std::fmt;

/// The revision of a tenant's runtime binding.
///
/// Every binding carries one, and it only ever moves forward. That single
/// property is what makes the rest of the lifecycle safe:
///
/// - **Cache invalidation** — a cached binding is stale if a higher revision
///   exists, which is cheaper and less race-prone than time-based expiry.
/// - **Out-of-order updates** — a refresh that arrives late carries an older
///   revision and is discarded rather than resurrecting a retired binding.
/// - **Migration coordination** — the cut-over from a shared database to a
///   dedicated one is exactly "publish revision N+1", so pools attached to
///   revision N can be drained once no request holds them.
/// - **Diagnostics** — the revision is emitted on every request, so a support
///   question becomes "which binding revision served this trace?".
///
/// Comparisons are the whole point of the type, so it derives [`Ord`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct BindingRevision(u64);

impl BindingRevision {
    /// The revision of a binding that has never been published.
    ///
    /// Used as the floor when the registry decides whether an incoming update
    /// is newer than what it already holds.
    pub const ZERO: Self = Self(0);

    /// Wraps a raw revision number, typically one supplied by reconciliation.
    #[must_use]
    pub const fn new(revision: u64) -> Self {
        Self(revision)
    }

    /// Returns the raw revision number, for telemetry and wire formats.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the next revision.
    ///
    /// Saturating rather than wrapping: at `u64::MAX` the platform has bigger
    /// problems than a stuck revision, and wrapping would silently make a new
    /// binding look older than the one it replaces.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for BindingRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_order_by_their_numeric_value() {
        assert!(BindingRevision::new(41) < BindingRevision::new(42));
    }

    #[test]
    fn zero_is_the_floor_for_a_never_published_binding() {
        assert_eq!(BindingRevision::ZERO, BindingRevision::new(0));
        assert!(BindingRevision::ZERO < BindingRevision::new(1));
    }

    #[test]
    fn next_saturates_rather_than_wrapping_backwards() {
        let ceiling = BindingRevision::new(u64::MAX);
        assert_eq!(ceiling.next(), ceiling);
    }
}
