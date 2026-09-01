//! Where this instance keeps its own state, and where clients' secrets live.

use std::sync::Arc;

use fabric_control_plane::{
    ClientSecrets, InMemoryIntegrationStore, InMemorySecretStore, IntegrationStore, SecretStore,
};
use fabric_core::Clock;
use fabric_openbao::{OpenBao, OpenBaoClientSecrets, OpenBaoIntegrationStore, OpenBaoSecretStore};

use crate::config::{ControlPlaneAppConfig, SecretStoreConfig};

/// The two stores this instance keeps its own state in.
///
/// Named because they are always built together and always from the same
/// client: the secrets and the record share a login, and separating them into
/// two constructions would mean two logins for one process.
pub(super) type InstanceStores = (
    Arc<dyn SecretStore>,
    Arc<dyn IntegrationStore>,
    Option<Arc<dyn ClientSecrets>>,
);

/// Builds the stores: this instance's own two, and clients' secrets.
///
/// Once, and shared by every flow above. One client means one login and one
/// cached token; building them per flow would mean a login per capability,
/// and a token refreshing three times over for no gain.
pub(super) fn build(
    config: &ControlPlaneAppConfig,
    clock: &Arc<dyn Clock>,
) -> Result<InstanceStores, String> {
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
                Arc::new(OpenBaoIntegrationStore::new(Arc::clone(&client))),
                // The same client again, so one login serves clients' secrets
                // as well as this instance's own state.
                Some(Arc::new(OpenBaoClientSecrets::new(client))),
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
                // No in-memory stand-in on purpose. A development store for
                // clients' secrets would let the console demonstrate managing
                // something that is not kept anywhere, which is a worse lie
                // than the tab saying it is not configured.
                None,
            ))
        }
    }
}
