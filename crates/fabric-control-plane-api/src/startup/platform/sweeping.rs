//! The background pass that advances an environment.

use std::sync::Arc;
use std::time::Duration;

use fabric_control_plane::PlatformBinding;
use fabric_platform_management::{PlatformManagement, SweepResult, SweepState};

use crate::config::PlatformManagementConfig;

/// Starts the sweep, if this deployment has one to run.
///
/// # Why the host starts this and the crate does not
///
/// The same reason the client reconciliation loop is the host's: a process
/// that only wants to serve the API — a test, a replica running read-only —
/// should be able to have one without a background task advancing
/// environments behind it. The rules crate takes an environment and a state
/// and is not a scheduler; the cadence is a deployment's to choose.
///
/// A zero interval, or no configuration at all, starts nothing. That is how a
/// deployment observes an environment without advancing it.
pub fn start_sweeping(
    config: Option<&PlatformManagementConfig>,
    platform: Option<&PlatformBinding>,
    sweeps: &Arc<SweepState>,
) {
    let (Some(config), Some(platform)) = (config, platform) else {
        return;
    };

    if config.reconciliation_interval_seconds == 0 {
        tracing::info!(
            environment = config.environment,
            "platform management is configured and its sweep is disabled"
        );
        return;
    }

    let environment = platform.environment.clone();
    let interval = Duration::from_secs(config.reconciliation_interval_seconds);
    let platform = Arc::clone(&platform.service);
    let sweeps = Arc::clone(sweeps);

    tracing::info!(
        environment,
        interval_seconds = config.reconciliation_interval_seconds,
        "platform management will advance this environment"
    );

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // The first tick fires immediately, which is what an operator expects
        // after a restart: the environment is checked now, not in a minute.
        ticker.tick().await;

        loop {
            sweep_once(&platform, &environment, &sweeps).await;
            ticker.tick().await;
        }
    });
}

/// One pass, with its outcome logged.
///
/// A failure here is recorded and the loop continues. The reason it must not
/// end is the same reason a sweep does not abandon its remaining components: a
/// registry being briefly unreachable would otherwise stop an environment
/// advancing until somebody restarted the process, and nothing would say so.
async fn sweep_once(platform: &Arc<PlatformManagement>, environment: &str, sweeps: &Arc<SweepState>) {
    match platform.sweep(environment, sweeps).await {
        Ok(SweepResult::NotConnected) => {
            // Every tick until an operator connects one. Not logged at all:
            // a minute's interval would fill a log with the fact that nobody
            // has done something yet, and the console already says so.
        }
        Ok(SweepResult::AlreadyRunning) => {
            tracing::warn!(
                environment,
                "a platform sweep overran its interval and was skipped"
            );
        }
        Ok(SweepResult::Ran(sweep)) => {
            for (component, swept) in &sweep.components {
                if let fabric_platform_management::Swept::Advanced { from, to } = swept {
                    tracing::info!(environment, component, %from, %to, "advanced");
                }
            }
        }
        Err(error) => {
            tracing::warn!(environment, error = %error, "a platform sweep could not run");
        }
    }
}
