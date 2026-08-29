//! The background loop that keeps providers converged.

use std::sync::Arc;
use std::time::Duration;

use fabric_core::Clock;
use fabric_reconciliation::{IdentityReconciler, ReconciliationStatusStore};
use tokio::sync::Notify;

use crate::integration::IntegrationHealth;
use crate::reconcile::{handle::ReconciliationLoopHandle, pass, ReconciliationTrigger};
use crate::repository::DesiredStateBinding;
use crate::{logging, ReconciliationConfig};

/// Sweeps every client on an interval, and whenever asked.
///
/// # This is the only thing that talks to a platform service
///
/// No HTTP handler in this crate reaches an identity provider. The loop does,
/// on its own schedule, reading desired state that has already been committed.
/// That separation is what makes Git the authority rather than one of two
/// writers: an operator's request changes a document, and the provider follows
/// — never the other way round (ADR 0008).
pub struct ReconciliationLoop;

impl ReconciliationLoop {
    /// Starts the loop.
    ///
    /// A sweep runs immediately rather than after the first interval, so a
    /// freshly started control plane knows where every client stands within
    /// seconds instead of showing a screen full of `pending` for a minute.
    #[must_use]
    pub fn spawn(
        repository: Arc<DesiredStateBinding>,
        reconciler: Arc<IdentityReconciler>,
        statuses: Arc<ReconciliationStatusStore>,
        health: Arc<IntegrationHealth>,
        trigger: Arc<ReconciliationTrigger>,
        clock: Arc<dyn Clock>,
        config: &ReconciliationConfig,
    ) -> ReconciliationLoopHandle {
        let interval = Duration::from_secs(config.interval_seconds);
        let shutdown = Arc::new(Notify::new());
        let task_shutdown = Arc::clone(&shutdown);

        logging::reconciliation_started(&repository.current().describe(), config.interval_seconds);

        let task = tokio::spawn(async move {
            loop {
                pass::run(
                    repository.current().as_ref(),
                    reconciler.as_ref(),
                    statuses.as_ref(),
                    health.as_ref(),
                    clock.as_ref(),
                )
                .await;

                tokio::select! {
                    () = tokio::time::sleep(interval) => {}
                    () = trigger.requested() => {}
                    () = task_shutdown.notified() => break,
                }
            }

            logging::reconciliation_stopped();
        });

        ReconciliationLoopHandle::new(shutdown, task)
    }
}
