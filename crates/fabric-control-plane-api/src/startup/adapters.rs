//! Choosing the adapters this deployment runs with.

use std::sync::Arc;

use fabric_client_git::{GitClientRepository, GitCredential};
use fabric_control_plane::{ClientRepository, InMemoryClientRepository};
use fabric_core::Clock;
use fabric_keycloak::{AdminCredential, KeycloakIdentityProvider};
use fabric_reconciliation::testing::FakeIdentityProvider;
use fabric_reconciliation::IdentityProvider;

use crate::config::{DesiredStateConfig, IdentityProviderConfig};
use crate::secrets;
use crate::startup::local_documents;

/// Builds the desired-state repository this deployment is configured for.
///
/// # Errors
///
/// Returns a message if the repository cannot be built — an invalid URL, a
/// missing credential, or a local directory that cannot be read.
pub(super) async fn desired_state(config: &DesiredStateConfig) -> Result<Arc<dyn ClientRepository>, String> {
    match config {
        DesiredStateConfig::Git(git) => {
            let credential = GitCredential::new(secrets::resolve(&git.token_ref)?);

            Ok(Arc::new(GitClientRepository::new(git, credential)?))
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

            Ok(repository)
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
