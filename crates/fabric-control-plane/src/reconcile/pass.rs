//! One sweep over every client.

use fabric_core::Clock;
use fabric_reconciliation::{IdentityReconciler, ReconciliationStatusStore};

use crate::integration::{IntegrationHealth, Observation};
use crate::logging;
use crate::repository::ClientRepository;

/// Reconciles every client the repository holds, and records what happened.
///
/// # One failing client does not stop the sweep
///
/// Each client is reconciled and recorded independently, so a provider that
/// refuses one realm does not leave every other client's status frozen at
/// whatever it was. This matters more than it looks: the alternative shows a
/// stale `applied` for twenty healthy clients because the twenty-first is
/// broken, which is the failure mode that makes a status display untrustworthy.
///
/// # A failed sweep changes no desired state
///
/// Nothing here writes to the repository. Reconciliation reads desired state
/// and converges a provider onto it; if the provider cannot be converged, Git
/// is exactly as it was and the next sweep starts from the same documents.
///
/// Returns the number of clients reconciled.
pub(super) async fn run(
    repository: &dyn ClientRepository,
    reconciler: &IdentityReconciler,
    statuses: &ReconciliationStatusStore,
    health: &IntegrationHealth,
    clock: &dyn Clock,
) -> usize {
    let clients = match repository.list().await {
        Ok(clients) => {
            health.record(Observation::Read, clock.now_unix_seconds());
            clients
        }
        Err(error) => {
            // Deliberately leaves every recorded status untouched. A briefly
            // unreadable repository is not evidence that anything changed.
            //
            // The *integration's* health is a different question and is
            // recorded, because "the platform cannot read desired state" is
            // exactly what the operator console has to be able to show.
            health.record(Observation::of(&error), clock.now_unix_seconds());
            logging::sweep_failed(&repository.describe(), &error);
            return 0;
        }
    };

    let mut swept = 0;

    for stored in &clients {
        let client = stored.document.client();
        let outcome = reconciler.reconcile(client).await;
        let status = statuses.record(
            &client.id,
            stored.revision.clone(),
            &outcome,
            clock.now_unix_seconds(),
        );

        logging::client_reconciled(&client.id, status, outcome.actions());
        swept += 1;
    }

    swept
}
