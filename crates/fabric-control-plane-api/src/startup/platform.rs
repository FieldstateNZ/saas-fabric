//! Composing Platform Management, and starting what advances an environment.

use std::sync::Arc;

use fabric_control_plane::PlatformBinding;
use fabric_core::Clock;
use fabric_platform_management::{
    ChartIndex, DesiredState, PlatformDesiredState, PlatformManagement, Registry,
};
use fabric_registry::{HelmCharts, OciRegistry};

mod sweeping;

pub(super) use sweeping::start_sweeping;

use crate::config::PlatformManagementConfig;

/// Builds Platform Management, if this deployment does platform management.
///
/// # What is configuration, and what is not any more
///
/// The environment, the registry and the cadence are a deployment's: they are
/// facts about where this control plane runs. The *repository* and its
/// credential are not — an operator installs the Platform Management GitHub
/// App and picks a repository, and the platform stores what it learns doing
/// so.
///
/// So there is nothing to build a repository from at startup, and the binding
/// starts unconnected. A control plane that refused to start without one could
/// not be used to connect one.
///
/// # `None` still means unconfigured, and still never means "could not build"
///
/// A deployment that states this section and gets *its own* configuration
/// wrong fails to start. That rule has not moved; what moved is which things
/// are configuration. An absent integration is now legitimate runtime state
/// rather than a misconfiguration, and it is reported rather than fatal.
///
/// # Errors
///
/// Returns a message naming the field. Never a credential.
pub fn establish(
    config: Option<&PlatformManagementConfig>,
    clock: &Arc<dyn Clock>,
) -> Result<Option<PlatformBinding>, String> {
    let Some(config) = config else {
        return Ok(None);
    };

    let registry = OciRegistry::new(
        &config.registry.base_url,
        &config.registry.host,
        config.registry.http_timeout_seconds,
    )?;

    // Anonymous, like the image registry, and for the same reason: a chart
    // repository serves its index to anybody, so there is no credential here
    // to be conflated with the platform application's authority.
    let charts = HelmCharts::new(config.registry.http_timeout_seconds)?;

    let repository = PlatformDesiredState::unconnected();

    Ok(Some(PlatformBinding {
        service: Arc::new(PlatformManagement::new(
            Arc::new(registry) as Arc<dyn Registry>,
            Arc::new(charts) as Arc<dyn ChartIndex>,
            Arc::clone(&repository) as Arc<dyn DesiredState>,
            Arc::clone(clock),
        )),
        repository,
        environment: config.environment.clone(),
    }))
}
