//! The facts the readiness decision is taken over.
//!
//! Plain counts and flags, gathered by the probe and judged by
//! [`readiness_state`](crate::health::readiness_state). Keeping them in a type
//! of their own is what lets the decision be tested without a runtime, a
//! registry, or a connector anywhere in sight — and, more usefully, what stops
//! the decision quietly growing a dependency on something it should not be
//! reading.

use crate::health::connector_health::ConnectorOutcome;

/// What one reconciled registry contributes to the decision.
pub(super) struct RegistryFacts {
    /// Whether a snapshot has ever loaded. Priming is irreversible.
    pub(super) primed: bool,

    /// How many resources the snapshot holds.
    ///
    /// Load-bearing, not decoration: a registry can be primed and empty, since
    /// a source that publishes an honestly empty set is a legitimate first
    /// load. "It loaded" and "it holds something usable" are different
    /// questions and the probe needs both.
    pub(super) count: usize,
}

/// What the connector sweep contributes to the decision.
pub(super) struct ConnectorFacts {
    /// How many connectors are registered.
    pub(super) total: usize,

    /// How many answered that they are serviceable.
    pub(super) healthy: usize,

    /// How many had not answered when the sweep's budget expired.
    ///
    /// Counted separately from the unhealthy ones on purpose — see
    /// [`ConnectorHealth`](crate::health::connector_health::ConnectorHealth).
    pub(super) unknown: usize,
}

impl From<&[ConnectorOutcome]> for ConnectorFacts {
    fn from(outcomes: &[ConnectorOutcome]) -> Self {
        Self {
            total: outcomes.len(),
            healthy: outcomes
                .iter()
                .filter(|outcome| outcome.health.is_healthy())
                .count(),
            unknown: outcomes
                .iter()
                .filter(|outcome| outcome.health.is_unknown())
                .count(),
        }
    }
}
