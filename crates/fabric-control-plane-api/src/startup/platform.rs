//! Composing Platform Management, and starting what advances an environment.

use std::sync::Arc;
use std::time::Duration;

use fabric_control_plane::PlatformBinding;
use fabric_core::Clock;
use fabric_git_host::GitCredential;
use fabric_platform_git::{PlatformGitRepository, PlatformRepositoryConfig};
use fabric_platform_management::{DesiredState, PlatformManagement, Registry, SweepResult, SweepState};
use fabric_registry::OciRegistry;

use crate::config::PlatformManagementConfig;
use crate::secrets;

/// Builds Platform Management, if this deployment manages a platform
/// repository.
///
/// # `None` means unconfigured, never "could not be built"
///
/// A deployment that states this section and gets it wrong fails to start.
/// The alternative — falling back to unmanaged — would present a
/// misconfiguration as a deliberate choice, and the console would say "nothing
/// is managed" to an operator who had configured something. They would have no
/// way to tell that from a platform that was never meant to manage anything,
/// and the symptom would be an environment that quietly never advanced.
///
/// # Errors
///
/// Returns a message naming the field or the secret. Never a credential.
pub fn establish(
    config: Option<&PlatformManagementConfig>,
    clock: &Arc<dyn Clock>,
) -> Result<Option<PlatformBinding>, String> {
    let Some(config) = config else {
        return Ok(None);
    };

    let credential = GitCredential::token(secrets::resolve(&config.credential)?);

    let repository = PlatformGitRepository::new(
        &PlatformRepositoryConfig {
            api_base_url: config.repository.api_base_url.clone(),
            owner: config.repository.owner.clone(),
            repository: config.repository.name.clone(),
            branch: config.repository.branch.clone(),
            http_timeout_seconds: config.repository.http_timeout_seconds,
        },
        credential,
        Arc::clone(clock),
    )?;

    let registry = OciRegistry::new(
        &config.registry.base_url,
        &config.registry.host,
        config.registry.http_timeout_seconds,
    )?;

    Ok(Some(PlatformBinding {
        service: Arc::new(PlatformManagement::new(
            Arc::new(registry) as Arc<dyn Registry>,
            Arc::new(repository) as Arc<dyn DesiredState>,
            Arc::clone(clock),
        )),
        environment: config.repository.environment.clone(),
    }))
}

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
            environment = config.repository.environment,
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
