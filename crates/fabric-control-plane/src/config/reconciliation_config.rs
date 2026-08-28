//! How often the platform checks that reality matches Git.

/// Reconciliation loop settings.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ReconciliationConfig {
    /// Seconds between full sweeps.
    ///
    /// This bounds two different things, and the second is the one that sets
    /// the value. It bounds how long a *lost trigger* leaves a client showing
    /// `pending` — which barely matters, because triggers are rarely lost. It
    /// also bounds how long **drift** goes unnoticed: a realm changed outside
    /// SaaS Fabric is invisible until the next sweep observes it.
    ///
    /// Sixty seconds is a compromise between noticing that promptly and the
    /// cost of the sweep, which is one read of the identity provider per
    /// client. A deployment with hundreds of clients should raise it; one
    /// investigating drift can lower it.
    pub interval_seconds: u64,
}

impl Default for ReconciliationConfig {
    fn default() -> Self {
        Self { interval_seconds: 60 }
    }
}
