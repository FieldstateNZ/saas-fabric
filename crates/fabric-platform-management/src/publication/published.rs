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

use crate::publication::protocol::publish_with_retry;
use crate::publication::RuntimePublisher;

impl RuntimePublisher {
    /// Offers `snapshot` to the target, and on acceptance remembers the
    /// tenants its tenants document carries, for
    /// [`crate::PublicationState::published_tenants`].
    pub(super) async fn offer(
        &self,
        snapshot: RuntimeSnapshot,
    ) -> Result<(PublicationReport, RuntimeSnapshot), PublicationError> {
        let offered = publish_with_retry(self.target.as_ref(), snapshot).await;
        if let (Ok((_, accepted)), Ok(mut held)) = (&offered, self.accepted.lock()) {
            *held = Some(published_tenants(&accepted.tenants.payload));
        }

        offered
    }

    /// What the pass that just ran left accepted, if it was accepted.
    pub(super) fn take_accepted(&self) -> Option<BTreeMap<TenantId, BindingRevision>> {
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
