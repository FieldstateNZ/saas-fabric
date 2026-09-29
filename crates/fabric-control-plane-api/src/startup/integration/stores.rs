//! Where this instance keeps its own state, and where clients' secrets live.

use std::sync::Arc;

use fabric_control_plane::{
    ClientSecrets, InMemoryIntegrationStore, InMemoryRegistryStore, InMemorySecretStore, IntegrationStore,
    RegistryStore, SecretStore,
};
use fabric_core::Clock;
use fabric_openbao::{
    OpenBao, OpenBaoClientSecrets, OpenBaoIntegrationStore, OpenBaoRegistryStore, OpenBaoSecretStore,
};

use crate::config::{ControlPlaneAppConfig, SecretStoreConfig};

/// The stores this instance keeps its own state in, and clients' secrets.
///
/// Always built together and from one client: the secrets, the Git
/// integrations' records and the registry records share a login, and
/// separate constructions would mean a login for each.
pub(in crate::startup) struct InstanceStores {
    /// This instance's secret partition: private keys, registry tokens.
    pub secrets: Arc<dyn SecretStore>,

    /// The Git integrations' records.
    pub integrations: Arc<dyn IntegrationStore>,

    /// The image registries' records.
    pub registries: Arc<dyn RegistryStore>,

    /// Clients' secrets, when there is a store for them.
    pub client_secrets: Option<Arc<dyn ClientSecrets>>,
}

/// Builds the stores: this instance's own three, and clients' secrets.
///
/// Once, and shared by every flow above. One client means one login and one
/// cached token; building them per flow would mean a login per capability,
/// and a token refreshing four times over for no gain.
pub(in crate::startup) fn build(
    config: &ControlPlaneAppConfig,
    clock: &Arc<dyn Clock>,
) -> Result<InstanceStores, String> {
    match &config.secret_store {
        SecretStoreConfig::OpenBao(openbao) => {
            let client = Arc::new(OpenBao::new(openbao, Arc::clone(clock))?);

            tracing::info!(
                event = "control_plane.secret_store",
                store = %client.describe(),
                "keeping this instance's own state in the platform secret store"
            );

            Ok(InstanceStores {
                secrets: Arc::new(OpenBaoSecretStore::new(Arc::clone(&client))),
                integrations: Arc::new(OpenBaoIntegrationStore::new(Arc::clone(&client))),
                registries: Arc::new(OpenBaoRegistryStore::new(Arc::clone(&client))),
                client_secrets: Some(Arc::new(OpenBaoClientSecrets::new(client))),
            })
        }

        SecretStoreConfig::InMemory => {
            tracing::warn!(
                event = "control_plane.development_secret_store",
                "using a development secret store; a connected Git integration, its private key, \
                 and every registered registry and its credential are lost when this process stops"
            );

            Ok(InstanceStores {
                secrets: Arc::new(InMemorySecretStore::new()),
                integrations: Arc::new(InMemoryIntegrationStore::new()),
                registries: Arc::new(InMemoryRegistryStore::new()),
                // No in-memory stand-in on purpose. A development store for
                // clients' secrets would let the console demonstrate managing
                // something that is not kept anywhere, which is a worse lie
                // than the tab saying it is not configured.
                client_secrets: None,
            })
        }
    }
}
