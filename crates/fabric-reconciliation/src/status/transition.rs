//! Deciding what to call the result of a pass, given what came before.

use fabric_client_model::ClientRevision;

use crate::status::{ReconciliationReport, ReconciliationStatus};
use crate::ReconciliationOutcome;

/// Works out the status to record for a pass.
///
/// A pass on its own can only report success or failure. Whether a *successful*
/// pass that changed something was ordinary convergence or the correction of
/// **drift** depends entirely on history: correcting a provider that had never
/// been converged to this revision is the reconciler doing its job, while
/// correcting one that was already converged means something outside SaaS
/// Fabric changed a realm the platform owns.
///
/// That distinction is why this is a function of the previous report rather
/// than a property of the outcome. It is also why it is written down once,
/// here, instead of inside the status store: the store's job is to hold
/// reports, not to have opinions about them.
pub(super) fn status_for(
    previous: Option<&ReconciliationReport>,
    revision: &ClientRevision,
    outcome: &ReconciliationOutcome,
) -> ReconciliationStatus {
    if outcome.status() == ReconciliationStatus::Failed {
        return ReconciliationStatus::Failed;
    }

    if outcome.changed_nothing() {
        return ReconciliationStatus::Applied;
    }

    let corrected_a_converged_client = previous
        .is_some_and(|report| report.status == ReconciliationStatus::Applied && report.revision == *revision);

    if corrected_a_converged_client {
        ReconciliationStatus::Drifted
    } else {
        ReconciliationStatus::Applied
    }
}
