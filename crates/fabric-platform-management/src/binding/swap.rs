//! Replacing what is bound, once nothing is using what was.
//!
//! All three take the write guard, so all three wait. That is not a detail of
//! the implementation, it is the promise: an operator who is told the platform
//! has stopped targeting a repository has been told that nothing more will be
//! written there, and a caller mid-operation against the old one finishes
//! against the old one rather than half against each.
//!
//! # Nothing a caller does can cancel the wait
//!
//! The promise used to hold only for an operation that ran to completion or
//! failed. Three things could cancel one instead — an operator's browser
//! disconnecting, the API's request timeout, and the adapter's own operation
//! budget — and each dropped the future, which released the guard with the
//! operation's last request possibly already on the wire.
//!
//! None of the three can do that now. A caller that goes away drops nothing the
//! operation is running in, because the operation runs in a task of its own
//! that owns the guard (`binding/holding.rs`). And the budget refuses to
//! *start* a request it cannot afford rather than cancelling one it already
//! sent, so an operation always reaches an outcome — a failure is still an
//! outcome — inside the budget plus one call to the host.
//!
//! # The one thing left, which no caller can close
//!
//! A request the platform gave up waiting for is not a request the host gave up
//! applying. If the adapter's `http_timeout_seconds` fires on a ref update the
//! host is still processing, the platform is told the call failed and the host
//! may commit it a moment later — after this returns. That is a property of a
//! network, not of a lock: there is no answer to wait for and nothing to
//! withdraw. What the platform *reports* is honest either way, because it
//! reports the write as failed; the next read sees whatever landed.

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
