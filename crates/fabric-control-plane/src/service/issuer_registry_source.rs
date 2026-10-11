//! How the control plane answers [`IssuerRegistrySource`] (D01-4): every
//! client whose lifecycle permits routing, registered by its realm's issuer.

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_client_model::lifecycle::ReadinessCondition;
use fabric_client_model::{ClientLifecycle, LifecycleRefusal, LifecycleUse};
use fabric_core::TenantId;
use fabric_platform_management::{
    Excluded, Exclusion, IssuerRegistrySource, Registrations, RegistryEvidence, RegistrySourceError,
};
use fabric_runtime_publication::{IssuerOrigin, IssuerRegistration};

use crate::lifecycle::facts::{Placements, PlatformFacts, Publication};
use crate::ClientService;

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
pub(crate) struct LifecycleIssuerRegistry {
    service: Arc<ClientService>,
    origin: IssuerOrigin,
}

impl LifecycleIssuerRegistry {
    /// Registers issuers under `origin`, deciding readiness with `service`.
    pub(crate) fn new(service: Arc<ClientService>, origin: IssuerOrigin) -> Self {
        Self { service, origin }
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
            let lifecycle = crate::lifecycle::derive(&self.service, stored, &facts);
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
        registrations
            .excluded
            .extend(bound.into_keys().map(|tenant| Excluded {
                tenant,
                reason: Exclusion::NoClient,
            }));
        Ok(registrations)
    }
}

/// The routing refusal as an exclusion kind, so evidence still pending is
/// never reported as a recorded retirement. A not-ready refusal names each
/// unmet condition with the evidence the lifecycle holds for it.
fn exclusion(refusal: &LifecycleRefusal, lifecycle: &ClientLifecycle) -> Exclusion {
    match refusal {
        LifecycleRefusal::Retired { state, .. } => Exclusion::Retired {
            state: state.to_string(),
        },
        _ => Exclusion::NotReady {
            unmet: match lifecycle {
                ClientLifecycle::Incomplete { unmet } => unmet
                    .iter()
                    .map(|unmet| format!("{}: {}", condition(unmet.condition), unmet.detail))
                    .collect(),
                _ => vec![refusal.to_string()],
            },
        },
    }
}

const fn condition(condition: ReadinessCondition) -> &'static str {
    match condition {
        ReadinessCondition::Provisioning => "provisioning",
        ReadinessCondition::Identity => "identity",
        ReadinessCondition::Publication => "publication",
    }
}
