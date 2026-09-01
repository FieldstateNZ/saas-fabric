//! The two connection flows, and what each one binds when it connects.

use std::sync::Arc;
use std::time::Duration;

use fabric_client_git::{AppPurpose, GitHubAppProvisioning, GitRepositoryFactory};
use fabric_control_plane::{
    ClientConfigurationTarget, DesiredStateBinding, GitIntegrationService, IntegrationKind, IntegrationStore,
    IntegrationTarget, PlatformBinding, SecretStore,
};
use fabric_core::Clock;

use crate::config::ControlPlaneAppConfig;
use crate::startup::platform_target::PlatformManagementTarget;

/// Connecting the repository this platform reads and writes client
/// configuration in.
///
/// # Errors
///
/// Returns a message if a client for the Git host cannot be built.
pub(super) fn clients(
    config: &ControlPlaneAppConfig,
    desired_state: &Arc<DesiredStateBinding>,
    secrets: &Arc<dyn SecretStore>,
    store: &Arc<dyn IntegrationStore>,
    clock: &Arc<dyn Clock>,
) -> Result<Arc<GitIntegrationService>, String> {
    let host = &config.git_host;

    let factory = Arc::new(GitRepositoryFactory::new(
        &host.api_base_url,
        &host.committer_name,
        &host.committer_email,
        host.http_timeout_seconds,
        Arc::clone(clock),
    ));

    build(
        IntegrationKind::ClientConfiguration,
        // Both are already published: deployments have an application under
        // this name, and the Git host has these callbacks stored against it.
        // Neither may change to make room for the second flow.
        AppPurpose {
            name: "SaaS Fabric".to_owned(),
            callback_segment: "git".to_owned(),
        },
        config,
        Arc::new(ClientConfigurationTarget::new(factory, Arc::clone(desired_state))),
        secrets,
        store,
        clock,
    )
}

/// Connecting the repository this platform writes desired platform state in.
///
/// A second application, not a second use of the first. The two are installed
/// separately, on different repositories, by an operator who may reasonably
/// want one without the other — and an installation that reaches a client's
/// configuration has no business reaching the platform's.
///
/// # Errors
///
/// Returns a message if a client for the Git host cannot be built.
pub(super) fn platform(
    config: &ControlPlaneAppConfig,
    binding: &PlatformBinding,
    secrets: &Arc<dyn SecretStore>,
    store: &Arc<dyn IntegrationStore>,
    clock: &Arc<dyn Clock>,
) -> Result<Arc<GitIntegrationService>, String> {
    let host = &config.git_host;

    build(
        IntegrationKind::PlatformManagement,
        AppPurpose {
            name: "SaaS Fabric Platform".to_owned(),
            callback_segment: "platform".to_owned(),
        },
        config,
        Arc::new(PlatformManagementTarget::new(
            host.api_base_url.clone(),
            host.http_timeout_seconds,
            Arc::clone(clock),
            Arc::clone(&binding.repository),
        )),
        secrets,
        store,
        clock,
    )
}

/// What the two have in common: the same Git host, the same stores, and a
/// kind that decides which record in them is theirs.
fn build(
    kind: IntegrationKind,
    purpose: AppPurpose,
    config: &ControlPlaneAppConfig,
    target: Arc<dyn IntegrationTarget>,
    secrets: &Arc<dyn SecretStore>,
    store: &Arc<dyn IntegrationStore>,
    clock: &Arc<dyn Clock>,
) -> Result<Arc<GitIntegrationService>, String> {
    let host = &config.git_host;

    let provisioning = Arc::new(GitHubAppProvisioning::new(
        &host.api_base_url,
        &host.web_base_url,
        &config.control_plane.public_base_url,
        Duration::from_secs(host.http_timeout_seconds),
        purpose,
    )?);

    Ok(Arc::new(GitIntegrationService::new(
        kind,
        provisioning,
        Arc::clone(secrets),
        Arc::clone(store),
        target,
        Arc::clone(clock),
    )))
}
