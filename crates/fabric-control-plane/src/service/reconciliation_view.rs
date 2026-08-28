//! Turning a reconciliation report into what an operator should be told.

use fabric_client_model::ClientRevision;
use fabric_reconciliation::{ReconciliationReport, ReconciliationStatus};

use crate::models::ReconciliationResponse;

/// Works out the status to show for a client at a given revision.
///
/// # A report about an older revision is not news about this one
///
/// The rule that matters here: a report is only meaningful for the revision it
/// was made against. If an operator has written a new revision since, the
/// honest answer is `pending` — the provider has provably not been checked
/// against what Git now says — even though a report exists and says `applied`.
///
/// Getting that wrong is how a control plane shows a green tick over a change
/// that has not happened. It is also why the failure detail is dropped along
/// with the status: showing last week's error message beside this minute's
/// desired state is worse than showing nothing, because it reads as current.
pub(super) fn resolve(
    report: Option<&ReconciliationReport>,
    current: &ClientRevision,
) -> ReconciliationResponse {
    match report {
        Some(report) if report.revision == *current => ReconciliationResponse {
            status: report.status,
            observed_at_unix: Some(report.observed_at_unix),
            detail: report.detail.clone(),
        },

        // Either nothing has been reconciled for this client at all, or what
        // was reconciled is no longer what is wanted. Both are pending.
        _ => ReconciliationResponse {
            status: ReconciliationStatus::Pending,
            observed_at_unix: None,
            detail: None,
        },
    }
}
