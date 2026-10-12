//! Deriving each client's lifecycle from what is recorded and what the
//! control plane can see.

use fabric_client_model::lifecycle::Readiness;
use fabric_client_model::ClientLifecycle;
use fabric_core::TenantId;
use fabric_platform_management::PlacementState;

use super::evidence;
use super::facts::{Placements, PlatformFacts, Publication};
use super::withdrawal::withdrawal;
use crate::models::LifecycleResponse;
use crate::state::ControlPlaneState;
use crate::{ClientService, StoredClient};

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
        let lifecycle = derive(&self.service, stored, facts);
        let retired = matches!(
            lifecycle,
            ClientLifecycle::Tombstoned { .. } | ClientLifecycle::DeletionConfirmed { .. }
        );
        let tenant = TenantId::try_new(client.id.as_str()).ok();
        let withdrawal = retired.then(|| withdrawal(tenant.as_ref(), &facts.publication));

        LifecycleResponse::new(lifecycle, withdrawal)
    }

    async fn platform_facts(&self) -> PlatformFacts {
        let Some(platform) = &self.platform else {
            return PlatformFacts::not_managed();
        };
        let placements = match platform.repository.read_placements(&platform.environment).await {
            Ok(read) => Placements::Read(read.placements),
            Err(_) => Placements::Unreadable,
        };
        let publication = match &platform.publisher {
            None => Publication::NotConfigured,
            Some(_) => Publication::Observed {
                last_pass: platform.publication.last_pass(),
                published: platform.publication.published_tenants(),
            },
        };

        PlatformFacts {
            placements,
            publication,
        }
    }
}

/// One client's lifecycle against `facts`: what is recorded, and the
/// evidence for each readiness condition. The issuer registry decides which
/// clients may be routed to with this same derivation (D01-4).
pub(crate) fn derive(
    service: &ClientService,
    stored: &StoredClient,
    facts: &PlatformFacts,
) -> ClientLifecycle {
    let client = stored.document.client();
    let readiness = Readiness::new(
        evidence::provisioning(client, facts),
        evidence::identity(
            service.reconciliation(stored).status,
            &stored.document.projection_check(),
        ),
        evidence::publication(client, facts),
    );
    ClientLifecycle::derive(&client.lifecycle, readiness)
}
