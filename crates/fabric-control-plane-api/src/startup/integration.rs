//! Assembling the Git connection flow, when this deployment has one.

use std::sync::Arc;
use std::time::Duration;

use fabric_client_git::{GitHubAppProvisioning, GitRepositoryFactory};
use fabric_control_plane::{
    DesiredStateBinding, GitIntegrationService, InMemoryIntegrationStore, InMemorySecretStore,
    IntegrationStore, SecretStore,
};
use fabric_core::Clock;
use fabric_openbao::{OpenBao, OpenBaoIntegrationStore, OpenBaoSecretStore};

use crate::config::{ControlPlaneAppConfig, DesiredStateConfig, SecretStoreConfig};

/// Builds the connection flow, or nothing when the deployment states its
/// repository itself.
///
/// # Errors
///
/// Returns a message if a client for the Git host or the secret store cannot
/// be built. **Not** if either is unreachable: this runs at startup, and a
/// control plane that refuses to start because a dependency is down cannot be
/// used to find out why.
pub(super) async fn establish(
    config: &ControlPlaneAppConfig,
    binding: &Arc<DesiredStateBinding>,
    clock: &Arc<dyn Clock>,
) -> Result<Option<Arc<GitIntegrationService>>, String> {
    // A deployment that names its repository has opted out. Offering it a flow
    // that would overwrite that from a browser would be offering to undo it.
    if !matches!(config.desired_state, DesiredStateConfig::Managed) {
        return Ok(None);
    }

    if config.control_plane.public_base_url.trim().is_empty() {
        return Err(
            "control_plane.public_base_url must be set to connect a Git integration: the Git \
             host returns the operator's browser to it"
                .to_owned(),
        );
    }

    let (secrets, store) = stores(config, clock)?;
    let host = &config.git_host;

    let provisioning = Arc::new(GitHubAppProvisioning::new(
        &host.api_base_url,
        &host.web_base_url,
        &config.control_plane.public_base_url,
        Duration::from_secs(host.http_timeout_seconds),
    )?);

    let factory = Arc::new(GitRepositoryFactory::new(
        &host.api_base_url,
        &host.committer_name,
        &host.committer_email,
        host.http_timeout_seconds,
        Arc::clone(clock),
    ));

    let service = Arc::new(GitIntegrationService::new(
        provisioning,
        secrets,
        store,
        factory,
        Arc::clone(binding),
        Arc::clone(clock),
    ));

    // Picks up whatever an operator connected before the last restart. Fails
    // at nothing: an unreachable store means the platform reports itself
    // unconfigured, which is honest, and the console still loads.
    service.restore().await;

    Ok(Some(service))
}

/// The two stores this instance keeps its own state in.
///
/// Named because they are always built together and always from the same
/// client: the secrets and the record share a login, and separating them into
/// two constructions would mean two logins for one process.
type InstanceStores = (Arc<dyn SecretStore>, Arc<dyn IntegrationStore>);

/// Builds the two stores this instance keeps its own state in.
fn stores(config: &ControlPlaneAppConfig, clock: &Arc<dyn Clock>) -> Result<InstanceStores, String> {
    match &config.secret_store {
        SecretStoreConfig::OpenBao(openbao) => {
            // One client for both stores, so one login serves both and the
            // token is cached once rather than twice.
            let client = Arc::new(OpenBao::new(openbao, Arc::clone(clock))?);

            tracing::info!(
                event = "control_plane.secret_store",
                store = %client.describe(),
                "keeping this instance's own state in the platform secret store"
            );

            Ok((
                Arc::new(OpenBaoSecretStore::new(Arc::clone(&client))),
                Arc::new(OpenBaoIntegrationStore::new(client)),
            ))
        }

        SecretStoreConfig::InMemory => {
            tracing::warn!(
                event = "control_plane.development_secret_store",
                "using a development secret store; a connected Git integration and its private \
                 key are lost when this process stops"
            );

            Ok((
                Arc::new(InMemorySecretStore::new()),
                Arc::new(InMemoryIntegrationStore::new()),
            ))
        }
    }
}
