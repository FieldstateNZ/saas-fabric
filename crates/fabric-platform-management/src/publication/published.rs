//! Offering a snapshot, and remembering what tenants document the
//! publication target accepted.

#[cfg(test)]
#[path = "published_tests.rs"]
mod published_tests;

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, TenantId};
use fabric_runtime_publication::{
    PublicationError, PublicationReport, RuntimeSnapshot, TenantBindingDocument,
};

use crate::publication::issuer_registry_source::Excluded;
use crate::publication::protocol::publish_with_retry;
use crate::publication::readiness_evidence::ReadinessEvidence;
use crate::publication::RuntimePublisher;

impl RuntimePublisher {
    /// Offers `snapshot` to the target, and on acceptance remembers the
    /// tenants its tenants document carries, for
    /// [`crate::PublicationState::published_tenants`], and what the
    /// readiness report reads, with the tenants the registry `excluded`.
    pub(super) async fn offer(
        &self,
        snapshot: RuntimeSnapshot,
        excluded: Vec<Excluded>,
    ) -> Result<(PublicationReport, RuntimeSnapshot), PublicationError> {
        let offered = publish_with_retry(self.target.as_ref(), snapshot).await;
        if let (Ok((report, accepted)), Ok(mut held)) = (&offered, self.accepted.lock()) {
            let registry = report.registry.map(|registry| registry.generation);
            *held = Some(Accepted {
                tenants: published_tenants(&accepted.tenants.payload),
                evidence: ReadinessEvidence::of(accepted, (registry, report.record), excluded),
            });
        }

        offered
    }

    /// What the pass that just ran left accepted, if it was accepted.
    pub(super) fn take_accepted(&self) -> Option<Accepted> {
        self.accepted.lock().ok().and_then(|mut held| held.take())
    }
}

/// Each tenant an accepted tenants document carries, at its binding
/// revision.
pub(super) fn published_tenants(tenants: &[TenantBindingDocument]) -> BTreeMap<TenantId, BindingRevision> {
    tenants
        .iter()
        .map(|binding| (binding.tenant.clone(), binding.revision))
        .collect()
}

/// What one pass left accepted: the tenants document's tenants, and the
/// readiness evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    /// Each published tenant, at its binding revision.
    pub tenants: BTreeMap<TenantId, BindingRevision>,
    /// What the readiness report reads.
    pub evidence: ReadinessEvidence,
}
