//! Read access to state a reconciliation pass shares with every other
//! [`ClientService`] method — the store it records into, and the clock it
//! stamps outcomes with.

use fabric_core::Clock;
use fabric_reconciliation::ReconciliationStatusStore;

use crate::ClientService;

impl ClientService {
    /// What is known about whether desired state has taken effect.
    ///
    /// Exposed for the convergence pass, which records into the same store the
    /// read paths report from — two stores would be two answers to one
    /// question.
    #[must_use]
    pub(crate) fn statuses(&self) -> &ReconciliationStatusStore {
        &self.reconciliation
    }

    /// The clock, so a pass stamps outcomes the same way a write does.
    #[must_use]
    pub(crate) fn clock(&self) -> &dyn Clock {
        self.clock.as_ref()
    }
}
