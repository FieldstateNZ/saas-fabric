//! Replacing what is bound, once nothing is using what was.
//!
//! All three take the write guard, so all three wait. That is not a detail of
//! the implementation, it is the promise: an operator who is told the platform
//! has stopped targeting a repository has been told that nothing more will be
//! written there, and a caller mid-operation against the old one finishes
//! against the old one rather than half against each.

use std::sync::Arc;

use crate::binding::bound::Bound;
use crate::binding::PlatformDesiredState;
use crate::{DesiredState, SafeDiagnostic};

impl PlatformDesiredState {
    /// Points the platform at a repository, replacing whatever was there.
    ///
    /// Waits for every operation already running against the old repository,
    /// so nothing lands in it after this returns.
    pub async fn connect(&self, repository: Arc<dyn DesiredState>) {
        self.set(Bound::Repository(repository)).await;
    }

    /// Records that a connected integration could not be made to work.
    ///
    /// The text is sanitised here rather than by the caller, because here is
    /// where it becomes something an operator reads. What arrives is whatever
    /// the composition root observed trying to build a client; what leaves is
    /// a [`SafeDiagnostic`].
    pub async fn unusable(&self, detail: &str) {
        self.set(Bound::Unusable(SafeDiagnostic::sanitise(detail))).await;
    }

    /// Forgets the current repository.
    ///
    /// Used when an operator disconnects the integration, or when what was
    /// stored turns out to be unusable. The platform goes back to reporting
    /// itself unconnected rather than failing against something it can no
    /// longer reach.
    pub async fn disconnect(&self) {
        self.set(Bound::Nothing).await;
    }

    /// Replaces what is bound, and moves the generation on with it.
    async fn set(&self, bound: Bound) {
        self.current.write().await.replace(bound);
    }
}
