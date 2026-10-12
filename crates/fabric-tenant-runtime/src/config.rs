//! Configuration for the runtime plane.

use std::path::PathBuf;

/// How the runtime keeps its reconciled state current.
///
/// One setting pair for both registries. One loop refreshes them both, data
/// sources first, so a tenant binding is never reloaded ahead of the
/// DataSources published alongside it; separate knobs would reopen exactly
/// the window where a tenant binding references a DataSource the runtime has
/// not loaded or has already forgotten.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RuntimeConfig {
    /// How often the background refresher reloads, in seconds.
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

    /// A writable directory where the reader persists the last complete
    /// generation it read, so a restart while the published files are torn
    /// or mismatched can prime from it instead of failing (gap G5).
    ///
    /// Unset (the default) keeps the behaviour before the copy existed: no
    /// copy is written, and a restart with no complete published generation
    /// fails its prime. Must be writable by the process, survive a container
    /// restart (an `emptyDir` does; a pod rescheduled elsewhere starts empty
    /// unless it is a volume of its own), and must not be the directory the
    /// published files are in: the reader never writes where the publisher
    /// does.
    pub last_known_good_dir: Option<PathBuf>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            refresh_interval_seconds: 30,
            fail_fast_on_prime: true,
            last_known_good_dir: None,
        }
    }
}

impl RuntimeConfig {
    /// Checks the configuration before anything is built.
    ///
    /// # Errors
    ///
    /// Returns a message if the refresh interval is zero, which would spin the
    /// refresher in a tight loop against their sources.
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

    #[test]
    fn the_last_known_good_copy_is_off_unless_a_directory_is_configured() {
        assert_eq!(RuntimeConfig::default().last_known_good_dir, None);
    }
}
