//! Where the control plane keeps what it knows about reconciliation.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use fabric_client_model::{ClientId, ClientRevision};

use crate::status::{transition, ReconciliationReport, ReconciliationStatus};
use crate::ReconciliationOutcome;

/// The most recent reconciliation report for each client.
///
/// # In memory, and honestly so
///
/// This is process state, lost on restart. That is a deliberate first-increment
/// choice rather than an oversight, and it is safe for one reason: reconciliation
/// is idempotent and runs on a schedule, so a restarted control plane
/// re-observes every client and rebuilds the truth within one pass. What is
/// genuinely lost is *history* — that a client was `Drifted` an hour ago — and
/// a durable store is what a later increment adds when there is somewhere to
/// put it.
///
/// Nothing here is the source of truth for anything. Git holds desired state,
/// the provider holds actual state, and this holds an observation about the
/// relationship between them.
#[derive(Default)]
pub struct ReconciliationStatusStore {
    /// One report per client, keyed by client id.
    entries: Mutex<BTreeMap<ClientId, ReconciliationReport>>,
}

impl ReconciliationStatusStore {
    /// Builds an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The current report for a client, if one has been recorded.
    ///
    /// `None` means nothing is known yet — a client the reconciliation loop
    /// has not reached. The control plane presents that as `pending`, which is
    /// accurate: the provider has provably not been checked against this
    /// desired state.
    #[must_use]
    pub fn report(&self, client: &ClientId) -> Option<ReconciliationReport> {
        self.entries().get(client).cloned()
    }

    /// Records that a client's desired state has changed and has not been
    /// reconciled since.
    ///
    /// Called on every accepted write, before any provider is touched. This is
    /// the mechanism behind "a successful Git write does not mean the provider
    /// is updated" — the status says so from the moment the write lands.
    pub fn mark_pending(&self, client: &ClientId, revision: ClientRevision, at_unix: u64) {
        self.entries().insert(
            client.clone(),
            ReconciliationReport {
                status: ReconciliationStatus::Pending,
                revision,
                actions: 0,
                observed_at_unix: at_unix,
                detail: None,
            },
        );
    }

    /// Records the result of a reconciliation pass, and returns what it was
    /// called.
    ///
    /// The status is decided by `transition::status_for` against whatever was
    /// recorded before, which is how a correction to an already-converged
    /// client becomes `Drifted` rather than another ordinary `Applied`.
    pub fn record(
        &self,
        client: &ClientId,
        revision: ClientRevision,
        outcome: &ReconciliationOutcome,
        at_unix: u64,
    ) -> ReconciliationStatus {
        let mut entries = self.entries();
        let status = transition::status_for(entries.get(client), &revision, outcome);

        entries.insert(
            client.clone(),
            ReconciliationReport {
                status,
                revision,
                actions: outcome.actions(),
                observed_at_unix: at_unix,
                detail: outcome.detail().map(ToOwned::to_owned),
            },
        );

        status
    }

    /// Takes the lock, recovering from a poisoned one.
    ///
    /// A panic in another thread while this lock was held cannot have left the
    /// map inconsistent — every mutation above is a single `insert` — so
    /// refusing to serve reconciliation status for the rest of the process's
    /// life would be a strictly worse outcome than carrying on. This is also
    /// the only way to take the lock without `unwrap`, which the workspace
    /// denies.
    fn entries(&self) -> MutexGuard<'_, BTreeMap<ClientId, ReconciliationReport>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
