//! Choosing the adapters this deployment runs with.

use std::sync::Arc;

use fabric_client_git::{GitAuthConfig, GitClientRepository, GitCredential};
use fabric_control_plane::{DesiredStateBinding, InMemoryClientRepository};
use fabric_core::Clock;
use fabric_keycloak::{AdminCredential, KeycloakIdentityProvider};
use fabric_reconciliation::testing::FakeIdentityProvider;
use fabric_reconciliation::IdentityProvider;

use crate::config::{DesiredStateConfig, IdentityProviderConfig};
use crate::secrets;
use crate::startup::local_documents;

/// Builds the desired-state binding this deployment starts with.
///
/// A *binding* rather than a repository, because the production mode starts
/// with none: the platform reports itself unconfigured and stays available so
/// that an operator can connect one.
///
/// # Errors
///
/// Returns a message if a **stated** repository cannot be built — an invalid
/// URL, a missing credential, or a local directory that cannot be read. A
/// deployment that states where desired state lives and states it wrongly
/// still fails at startup: it has opted out of the managed path, and silently
/// starting unconfigured would hide the mistake behind a screen inviting
/// somebody to connect a repository the deployment already named.
pub(super) async fn desired_state(
    config: &DesiredStateConfig,
    clock: Arc<dyn Clock>,
) -> Result<Arc<DesiredStateBinding>, String> {
    match config {
        DesiredStateConfig::Managed => {
            tracing::info!(
                event = "control_plane.desired_state_unconfigured",
                "no desired-state repository yet; an operator connects one through the console"
            );

            Ok(DesiredStateBinding::unconfigured())
        }

        DesiredStateConfig::Git(git) => {
            // One secret either way; which one it is depends on the posture.
            // Resolving before the match keeps the failure "this secret is not
            // set" rather than one message per posture.
            let secret = secrets::resolve(git.auth.secret_ref())?;

            let credential = match &git.auth {
                GitAuthConfig::GithubApp {
                    app_id,
                    installation_id,
                    ..
                } => GitCredential::app(app_id, installation_id, secret),
                GitAuthConfig::Token { .. } => GitCredential::token(secret),
            };

            Ok(DesiredStateBinding::to(Arc::new(GitClientRepository::new(
                git, credential, clock,
            )?)))
        }

        DesiredStateConfig::LocalDirectory { path } => {
            let repository = Arc::new(InMemoryClientRepository::new());
            let loaded = local_documents::load(&repository, path).await?;

            // Loud, and at warn rather than info: a deployment that reached
            // this branch by accident is one whose operators' changes are
            // being written to a map that a restart will empty.
            tracing::warn!(
                event = "control_plane.development_desired_state",
                path = %path.display(),
                clients = loaded,
                "using a development desired-state repository; writes are kept in memory and \
                 are lost when this process stops"
            );

            Ok(DesiredStateBinding::to(repository))
        }
    }
}

/// Builds the identity provider this deployment is configured for.
///
/// # Errors
///
/// Returns a message if the provider cannot be built — invalid configuration,
/// or a missing credential.
pub(super) fn identity_provider(
    config: &IdentityProviderConfig,
    clock: Arc<dyn Clock>,
) -> Result<Arc<dyn IdentityProvider>, String> {
    match config {
        IdentityProviderConfig::Keycloak(keycloak) => {
            let credential = AdminCredential::new(secrets::resolve(&keycloak.client_secret_ref)?);

            Ok(Arc::new(KeycloakIdentityProvider::new(
                keycloak, credential, clock,
            )?))
        }

        IdentityProviderConfig::InMemory => {
            tracing::warn!(
                event = "control_plane.development_identity_provider",
                "using a development identity provider; reconciliation will report clients as \
                 converged without any identity provider being changed"
            );

            Ok(Arc::new(FakeIdentityProvider::new()))
        }
    }
}
