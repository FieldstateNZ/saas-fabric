//! What one application of reconciled state did.

/// The outcome of applying a set of resources to a registry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// Resources the registry had not seen before.
    pub added: usize,

    /// Resources whose revision advanced.
    pub updated: usize,

    /// Resources that disappeared from the source.
    pub removed: usize,

    /// Resources whose incoming revision was **older** than the one held, and
    /// were therefore ignored.
    pub stale_ignored: usize,

    /// Resources whose incoming copy matched what was already held.
    pub unchanged: usize,
}

impl ApplyReport {
    /// Whether anything actually moved.
    ///
    /// Used to decide between an info-level "snapshot applied" line and a
    /// debug-level "nothing changed" one. Most refreshes change nothing, and
    /// logging every one at info would bury the ones that matter.
    #[must_use]
    pub const fn is_noop(&self) -> bool {
        self.added == 0 && self.updated == 0 && self.removed == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_with_no_movement_is_a_noop() {
        let report = ApplyReport {
            unchanged: 12,
            stale_ignored: 1,
            ..ApplyReport::default()
        };

        assert!(report.is_noop());
    }

    #[test]
    fn any_movement_makes_it_not_a_noop() {
        assert!(!ApplyReport {
            removed: 1,
            ..ApplyReport::default()
        }
        .is_noop());
    }
}
