//! Assembling the Git connection flows, for whichever this deployment runs.

use std::sync::Arc;

use fabric_control_plane::{ClientSecrets, DesiredStateBinding, GitIntegrationService, PlatformBinding};
use fabric_core::Clock;

use crate::config::{ControlPlaneAppConfig, DesiredStateConfig};

mod services;
mod stores;

/// The connection flows this deployment runs, and what they keep state in.
///
/// Both flows are optional and independently so. A deployment that names its
/// own client repository still manages a platform; one that manages no
/// platform still connects clients. Nothing here couples the two beyond the
/// store they share.
pub(super) struct Integrations {
    /// Connecting a client-configuration repository, when an operator is the
    /// one who chooses it.
    pub clients: Option<Arc<GitIntegrationService>>,

    /// Connecting the platform repository, when this deployment manages one.
    pub platform: Option<Arc<GitIntegrationService>>,

    /// Where clients' secrets are read and written, when there is a store.
    pub client_secrets: Option<Arc<dyn ClientSecrets>>,
}

/// Builds the connection flows this deployment has, and the store beneath them.
///
/// # What decides whether each one exists
///
/// - **Clients**, unless the deployment states its repository itself. One that
///   does has opted out, and offering it a flow that would overwrite that from
///   a browser would be offering to undo it.
/// - **Platform**, whenever this deployment manages an environment. There is no
///   equivalent opt-out: the platform repository is only ever an operator's to
///   choose, so the flow is the only way it is ever set.
///
/// # Errors
///
/// Returns a message if a client for the Git host or the secret store cannot
/// be built. **Not** if either is unreachable: this runs at startup, and a
/// control plane that refuses to start because a dependency is down cannot be
/// used to find out why.
pub(super) async fn establish(
    config: &ControlPlaneAppConfig,
    desired_state: &Arc<DesiredStateBinding>,
    platform: Option<&PlatformBinding>,
    clock: &Arc<dyn Clock>,
) -> Result<Integrations, String> {
    let (secrets, store, client_secrets) = stores::build(config, clock)?;
    let connects_clients = matches!(config.desired_state, DesiredStateConfig::Managed);

    let mut integrations = Integrations {
        clients: None,
        platform: None,
        client_secrets,
    };

    if !connects_clients && platform.is_none() {
        return Ok(integrations);
    }

    if config.control_plane.public_base_url.trim().is_empty() {
        return Err(
            "control_plane.public_base_url must be set to connect a Git integration: the Git \
             host returns the operator's browser to it"
                .to_owned(),
        );
    }

    if connects_clients {
        integrations.clients = Some(services::clients(config, desired_state, &secrets, &store, clock)?);
    }

    // Both or neither: the binding exists exactly when the section does, and
    // taking the budget from the section rather than defaulting it here keeps
    // the value startup validated as the value that is used.
    if let (Some(binding), Some(managed)) = (platform, config.platform_management.as_ref()) {
        integrations.platform = Some(services::platform(
            config,
            binding,
            managed.operation_timeout_seconds,
            &secrets,
            &store,
            clock,
        )?);
    }

    // Picks up whatever an operator connected before the last restart. Fails
    // at nothing: an unreachable store means the platform reports itself
    // unconfigured, which is honest, and the console still loads.
    //
    // Each flow restores its own record, so one that has never been connected
    // does not stop the other from coming back.
    for service in [&integrations.clients, &integrations.platform]
        .into_iter()
        .flatten()
    {
        service.restore().await;
    }

    Ok(integrations)
}
