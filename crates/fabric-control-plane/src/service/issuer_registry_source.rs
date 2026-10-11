//! How the control plane answers [`IssuerRegistrySource`] (D01-4): every
//! client whose lifecycle permits routing, registered by its realm's issuer.

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_client_model::LifecycleUse;
use fabric_core::TenantId;
use fabric_platform_management::{
    IssuerRegistrySource, Registrations, RegistryEvidence, RegistrySourceError, Withheld,
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
/// (`LifecycleRefusal::Retired`) is left out, so no issuer reaches it.
/// A retired client's tenant binding stays published: withdrawal is
/// deferred (ADR 0021's 2026-10-11 amendment), and leaving the registry is
/// not evidence that any edge or runtime stopped serving it.
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
                .map_err(|refusal| refusal.to_string())
                .and_then(|()| {
                    self.origin
                        .issuer(client.identity.realm.as_str())
                        .map_err(|error| error.to_string())
                });
            match decided {
                Ok(issuer) => registrations
                    .registered
                    .push(IssuerRegistration { issuer, tenant }),
                Err(reason) if is_bound => registrations.withheld.push(Withheld { tenant, reason }),
                Err(_) => {}
            }
        }
        registrations
            .withheld
            .extend(bound.into_keys().map(|tenant| Withheld {
                tenant,
                reason: "no client in desired state names this tenant".to_owned(),
            }));
        Ok(registrations)
    }
}
