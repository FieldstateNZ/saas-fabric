//! What is known about whether desired state has taken effect.

use fabric_reconciliation::ReconciliationStatus;

/// Reconciliation state for one client, as an operator sees it.
///
/// # `observedAtUnix`, not a formatted timestamp
///
/// Seconds since the Unix epoch, formatted by whatever displays it. That keeps
/// every opinion about time zones and locale in the browser, where the reader
/// is, and keeps a date-time library out of the control plane for the sake of
/// one field.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReconciliationResponse {
    /// Where the client stands.
    pub(crate) status: ReconciliationStatus,

    /// When that was last established, in seconds since the Unix epoch.
    ///
    /// Absent when reconciliation has never run for this client, which is the
    /// honest answer — a zero would render as 1970 and read as a bug.
    pub(crate) observed_at_unix: Option<u64>,

    /// Why the last pass failed, if it did.
    ///
    /// Produced by the adapter that failed, which is required to keep upstream
    /// response bodies and credentials out of it. Present only when it
    /// describes the revision being shown, so an operator is never given a
    /// stale explanation next to fresh state.
    pub(crate) detail: Option<String>,
}
