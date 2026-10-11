//! Deriving each client's lifecycle from what is recorded and what the
//! control plane can see.

use fabric_client_model::lifecycle::Readiness;
use fabric_client_model::ClientLifecycle;
use fabric_platform_management::PlacementState;

use super::evidence::{self, PlatformFacts, Published};
use crate::models::LifecycleResponse;
use crate::state::ControlPlaneState;
use crate::StoredClient;

impl ControlPlaneState {
    /// One client's lifecycle.
    pub(crate) async fn lifecycle(&self, stored: &StoredClient) -> LifecycleResponse {
        let facts = self.platform_facts().await;

        self.derive(stored, &facts)
    }

    /// Every listed client's lifecycle, reading the platform once.
    pub(crate) async fn lifecycles(&self, clients: &[StoredClient]) -> Vec<LifecycleResponse> {
        let facts = self.platform_facts().await;

        clients.iter().map(|stored| self.derive(stored, &facts)).collect()
    }

    fn derive(&self, stored: &StoredClient, facts: &PlatformFacts) -> LifecycleResponse {
        let client = stored.document.client();
        let readiness = Readiness::new(
            evidence::provisioning(client, facts),
            evidence::identity(self.service.reconciliation(stored).status),
            evidence::publication(client, facts),
        );

        ClientLifecycle::derive(&client.lifecycle, readiness).into()
    }

    async fn platform_facts(&self) -> PlatformFacts {
        let Some(platform) = &self.platform else {
            return PlatformFacts::NotManaged;
        };
        let Ok(read) = platform.repository.read_placements(&platform.environment).await else {
            return PlatformFacts::Unreadable;
        };
        let published = match (&platform.publisher, platform.publication.published_tenants()) {
            (None, _) => Published::NoPublisher,
            (Some(_), None) => Published::NotYet,
            (Some(_), Some(published)) => Published::Known(published),
        };

        PlatformFacts::Read {
            placements: read.placements,
            published,
        }
    }
}
