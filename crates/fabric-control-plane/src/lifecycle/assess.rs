//! Deriving each client's lifecycle from what is recorded and what the
//! control plane can see.

use fabric_client_model::lifecycle::Readiness;
use fabric_client_model::ClientLifecycle;
use fabric_core::TenantId;
use fabric_platform_management::PlacementState;

use super::evidence;
use super::facts::{Placements, PlatformFacts, Publication};
use super::withdrawal::withdrawal;
use crate::continuity::{admit, admit_all, Admission};
use crate::models::LifecycleResponse;
use crate::state::ControlPlaneState;
use crate::{ClientService, StoredClient};

impl ControlPlaneState {
    /// One client's lifecycle.
    pub(crate) async fn lifecycle(&self, stored: &StoredClient) -> LifecycleResponse {
        let facts = self.platform_facts().await;
        let admission = admit(self.lifecycle_continuity.as_deref(), stored).await;

        self.derive(stored, &facts, admission)
    }

    /// Every listed client's lifecycle, reading the platform once.
    pub(crate) async fn lifecycles(&self, clients: &[StoredClient]) -> Vec<LifecycleResponse> {
        let facts = self.platform_facts().await;
        let admissions = admit_all(self.lifecycle_continuity.as_deref(), clients).await;

        clients
            .iter()
            .map(|stored| {
                let admission = admissions
                    .get(&stored.document.client().id)
                    .copied()
                    .unwrap_or(Admission::Held(crate::ContinuityHold::Unavailable));
                self.derive(stored, &facts, admission)
            })
            .collect()
    }

    fn derive(
        &self,
        stored: &StoredClient,
        facts: &PlatformFacts,
        admission: Admission,
    ) -> LifecycleResponse {
        let client = stored.document.client();
        let lifecycle = derive(&self.service, stored, facts, admission);
        let retired = matches!(
            lifecycle,
            ClientLifecycle::Tombstoned { .. } | ClientLifecycle::DeletionConfirmed { .. }
        );
        let tenant = TenantId::try_new(client.id.as_str()).ok();
        let withdrawal = retired.then(|| withdrawal(tenant.as_ref(), &facts.publication));

        LifecycleResponse::new(
            lifecycle,
            withdrawal,
            client.lifecycle.restorations.clone(),
            admission,
        )
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

/// One client's lifecycle against `facts` and the continuity store's
/// `admission`: what is recorded, and the evidence for each readiness
/// condition. The issuer registry decides which clients may be routed to
/// with this same derivation (D01-4).
pub(crate) fn derive(
    service: &ClientService,
    stored: &StoredClient,
    facts: &PlatformFacts,
    admission: Admission,
) -> ClientLifecycle {
    let client = stored.document.client();
    let readiness = Readiness::new(
        evidence::provisioning(client, facts),
        evidence::identity(
            service.reconciliation(stored).status,
            &stored.document.projection_check(),
            admission,
        ),
        evidence::publication(client, facts),
    );
    ClientLifecycle::derive(&client.lifecycle, readiness)
}
