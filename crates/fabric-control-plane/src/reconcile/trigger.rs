//! Asking for a reconciliation pass without waiting for the interval.

use tokio::sync::Notify;

/// A request for the reconciliation loop to run now.
///
/// # Why a trigger as well as an interval
///
/// The trigger is the fast path: an operator who has just changed a client's
/// roles should not watch a `pending` badge for a minute when the work takes
/// milliseconds.
///
/// The interval is the safety net, and it is the one that makes the design
/// correct rather than merely quick. Triggers get lost — the process restarts
/// between the write and the pass, a pass is already running and the next one
/// coalesces, the loop is wedged on a slow provider. If the trigger were the
/// only mechanism, a lost one would strand a client on `pending` forever with
/// nothing to notice it. With a poll, the worst case is bounded by the
/// interval regardless. This is the same argument the runtime plane's resource
/// refresher makes, for the same reason.
///
/// Requests coalesce: twenty writes in a burst produce one pass, which is what
/// you want when a pass reads every client anyway.
#[derive(Default)]
pub struct ReconciliationTrigger {
    /// The notification the loop waits on.
    notify: Notify,
}

impl ReconciliationTrigger {
    /// Builds a trigger nobody has pulled yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks for a pass as soon as the loop can run one.
    ///
    /// Returns immediately. If no loop is running — which is the case in tests
    /// and in a host that has not started one — this does nothing at all, and
    /// deliberately does not fail: a write must not be refused because a
    /// background task is absent.
    pub fn request_pass(&self) {
        self.notify.notify_one();
    }

    /// Waits for the next request.
    pub(crate) async fn requested(&self) {
        self.notify.notified().await;
    }
}
