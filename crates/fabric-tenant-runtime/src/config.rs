//! Configuration for the runtime plane.

/// How the runtime keeps its reconciled state current.
///
/// One setting pair for both registries. They have the same failure modes and
/// the same staleness tolerance, and giving them separate knobs would invite a
/// deployment where DataSources refresh every thirty seconds and tenants every
/// hour — which produces exactly the window where a tenant binding references a
/// DataSource the runtime has already forgotten.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RuntimeConfig {
    /// How often the background refreshers reload, in seconds.
    ///
    /// A *safety net*, not the primary propagation mechanism. A reconciler that
    /// knows something changed should say so — through `apply_one` or by
    /// triggering an immediate refresh — rather than waiting for the next poll.
    /// The interval bounds how stale things can get if that notification is
    /// lost.
    pub refresh_interval_seconds: u64,

    /// Whether a failed initial load should stop the process.
    ///
    /// `true` (the default) is right for most deployments: a process that
    /// cannot load its state can serve no tenant, and failing at startup
    /// surfaces the problem where a deployment pipeline will catch it.
    ///
    /// `false` lets the process start unprimed and return 503 until a refresh
    /// succeeds. Useful where a source may legitimately come up after the
    /// runtime.
    pub fail_fast_on_prime: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            refresh_interval_seconds: 30,
            fail_fast_on_prime: true,
        }
    }
}

impl RuntimeConfig {
    /// Checks the configuration before anything is built.
    ///
    /// # Errors
    ///
    /// Returns a message if the refresh interval is zero, which would spin the
    /// refreshers in a tight loop against their sources.
    pub fn validate(&self) -> Result<(), String> {
        if self.refresh_interval_seconds == 0 {
            return Err("tenant_runtime.refresh_interval_seconds must be greater than zero".to_owned());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_refresh_interval_is_rejected() {
        let config = RuntimeConfig {
            refresh_interval_seconds: 0,
            ..RuntimeConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn the_default_fails_fast_on_a_bad_prime() {
        assert!(RuntimeConfig::default().fail_fast_on_prime);
    }
}
