//! Configuration for the tenant runtime.

/// How the runtime keeps its bindings current.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TenantRuntimeConfig {
    /// How often the background refresher reloads bindings, in seconds.
    ///
    /// This is a *safety net*, not the primary propagation mechanism. A
    /// reconciler that knows a tenant changed should say so — through
    /// [`TenantRuntimeRegistry::apply_one`](crate::TenantRuntimeRegistry::apply_one)
    /// or by triggering an immediate refresh — rather than waiting for the next
    /// poll. The interval bounds how stale things can get if that notification
    /// is missed.
    ///
    /// Thirty seconds trades a little staleness for a lot of quiet: shorter
    /// intervals mostly produce no-op refreshes.
    pub refresh_interval_seconds: u64,

    /// Whether a failed initial load should stop the process.
    ///
    /// `true` (the default) is right for most deployments: a process that
    /// cannot load any bindings can serve no tenant, and failing at startup
    /// surfaces the problem where a deployment pipeline will catch it.
    ///
    /// Setting it to `false` lets the process start unprimed and return 503
    /// until a refresh succeeds. Useful where the binding source may legitimately
    /// come up after the runtime.
    pub fail_fast_on_prime: bool,
}

impl Default for TenantRuntimeConfig {
    fn default() -> Self {
        Self {
            refresh_interval_seconds: 30,
            fail_fast_on_prime: true,
        }
    }
}

impl TenantRuntimeConfig {
    /// Checks the configuration before anything is built.
    ///
    /// # Errors
    ///
    /// Returns a message if the refresh interval is zero, which would spin the
    /// refresher in a tight loop against the binding source.
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
        let config = TenantRuntimeConfig {
            refresh_interval_seconds: 0,
            ..TenantRuntimeConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn the_default_fails_fast_on_a_bad_prime() {
        assert!(TenantRuntimeConfig::default().fail_fast_on_prime);
    }
}
