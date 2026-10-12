//! How the control plane answers [`IssuerRegistrySource`] (D01-4): every
//! client whose lifecycle permits routing, registered by its realm's issuer.

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_client_model::LifecycleUse;
use fabric_core::TenantId;
use fabric_platform_management::{
    Excluded, Exclusion, IssuerRegistrySource, Registrations, RegistryEvidence, RegistrySourceError,
};
use fabric_runtime_publication::{IssuerOrigin, IssuerRegistration};

mod exclusion;

use exclusion::exclusion;

use crate::continuity::{admit, Admission};
use crate::lifecycle::facts::{Placements, PlatformFacts, Publication};
use crate::{ClientService, LifecycleContinuity};

/// Registers each client the publication's own generation shows ready.
///
/// The lifecycle is derived exactly as the console derives it, with one
/// difference: publication evidence is the tenants document being offered,
/// not the last one accepted. Registry and tenants document are one
/// generation, so a tenant is judged against the binding it is published
/// with (ADR 0019 G4a).
///
/// A client that is not ready (`LifecycleRefusal::NotReady`) or is retired
/// (`LifecycleRefusal::Retired`) is left out of the generated registry,
/// fail closed (#108, option (a)): identity evidence pending reconciliation
/// excludes it too, until it is reconciled. Exclusion is from what Fabric
/// publishes only. No runtime reads the registry and edge application is
/// platform composition's, so it is not withdrawal or access revocation. A
/// retired client's tenant binding stays published (ADR 0021's 2026-10-11
/// amendment).
///
/// A bound client the continuity store does not confirm cannot be decided
/// either way (O5, #108 comment 6120741272 item 2 and review 5489320331):
/// registering it would admit an unconfirmed identity, and leaving it out
/// would publish a registry decided without the authority. That includes a
/// retired client whose tombstone the store has not confirmed, and a bound
/// tenant whose client document is gone: a desired-state-only restriction
/// is not authority to withdraw what was accepted. The pass is held
/// instead, so the target keeps the record, registry, documents, bindings
/// and last-good readers it accepted.
pub(crate) struct LifecycleIssuerRegistry {
    service: Arc<ClientService>,
    origin: IssuerOrigin,
    continuity: Option<Arc<dyn LifecycleContinuity>>,
}

impl LifecycleIssuerRegistry {
    /// Registers issuers under `origin`, deciding readiness with `service`
    /// and admission with `continuity`.
    pub(crate) fn new(
        service: Arc<ClientService>,
        origin: IssuerOrigin,
        continuity: Option<Arc<dyn LifecycleContinuity>>,
    ) -> Self {
        Self {
            service,
            origin,
            continuity,
        }
    }
}

#[async_trait::async_trait]
impl IssuerRegistrySource for LifecycleIssuerRegistry {
    async fn registrations(
        &self,
        generation: &RegistryEvidence<'_>,
    ) -> Result<Registrations, RegistrySourceError> {
        let clients = self
            .service
            .list()
            .await
            .map_err(|error| RegistrySourceError::Unavailable(error.public_message()))?;
        let mut bound: BTreeMap<TenantId, _> = generation
            .tenants
            .iter()
            .map(|binding| (binding.tenant.clone(), binding.revision))
            .collect();
        let facts = PlatformFacts {
            placements: Placements::Read(generation.placements.to_vec()),
            publication: Publication::Offered(bound.clone()),
        };

        let mut registrations = Registrations::default();
        for stored in &clients {
            let client = stored.document.client();
            let Ok(tenant) = TenantId::try_new(client.id.as_str()) else {
                continue;
            };
            let is_bound = bound.remove(&tenant).is_some();
            let admission = admit(self.continuity.as_deref(), stored).await;
            if let (true, Admission::Held(hold)) = (is_bound, admission) {
                return Err(RegistrySourceError::Unavailable(format!(
                    "the issuer registry is held: {} is bound and its identity is {hold}",
                    client.id
                )));
            }
            let lifecycle = crate::lifecycle::derive(&self.service, stored, &facts, admission);
            let decided = lifecycle
                .permits(LifecycleUse::Route)
                .map_err(|refusal| exclusion(&refusal, &lifecycle))
                .and_then(|()| {
                    self.origin
                        .issuer(client.identity.realm.as_str())
                        .map_err(|error| Exclusion::InvalidIssuer {
                            detail: error.to_string(),
                        })
                });
            match decided {
                Ok(issuer) => registrations
                    .registered
                    .push(IssuerRegistration { issuer, tenant }),
                Err(reason) if is_bound => registrations.excluded.push(Excluded { tenant, reason }),
                Err(_) => {}
            }
        }
        if let Some(tenant) = bound.into_keys().next() {
            return Err(RegistrySourceError::Unavailable(format!(
                "the issuer registry is held: {tenant} is bound and has no client document, and nothing \
                 protected says its identity ended"
            )));
        }
        Ok(registrations)
    }
}
