//! What an accepted publication leaves for the readiness report to read.

use std::collections::BTreeSet;

use fabric_core::TenantId;
use fabric_runtime_publication::{PublishedRevisions, RegistryGeneration, RuntimeSnapshot};

use super::issuer_registry_source::Excluded;

/// What the last pass the target accepted left published, for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessEvidence {
    /// The held revision of each document, and the registry generation.
    pub revisions: PublishedRevisions,
    /// The registry's registered and excluded tenants, when one was
    /// produced.
    pub registry: Option<RegistryEvidenceSummary>,
    /// The connector every published DataSource names.
    pub connectors: BTreeSet<String>,
}

/// The registry a pass published, summarised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEvidenceSummary {
    /// Its generation.
    pub generation: RegistryGeneration,
    /// The tenants it registers.
    pub registered: BTreeSet<TenantId>,
    /// Bound tenants it leaves out, and why.
    pub excluded: Vec<Excluded>,
}

impl ReadinessEvidence {
    /// The evidence an accepted `snapshot` leaves: the revisions it was
    /// accepted at, the registry generation the target stamped, and the
    /// tenants the registry source excluded.
    #[must_use]
    pub fn of(
        snapshot: &RuntimeSnapshot,
        registry: Option<RegistryGeneration>,
        excluded: Vec<Excluded>,
    ) -> Self {
        let summary = snapshot
            .registry
            .as_ref()
            .zip(registry)
            .map(|(registered, generation)| RegistryEvidenceSummary {
                generation,
                registered: registered
                    .iter()
                    .map(|registration| registration.tenant.clone())
                    .collect(),
                excluded,
            });
        Self {
            revisions: PublishedRevisions {
                tenants: Some(snapshot.tenants.revision),
                data_sources: Some(snapshot.data_sources.revision),
                catalog: Some(snapshot.catalog.revision),
                registry,
            },
            registry: summary,
            connectors: snapshot
                .data_sources
                .payload
                .iter()
                .map(|data_source| data_source.connector.as_str().to_owned())
                .collect(),
        }
    }
}
