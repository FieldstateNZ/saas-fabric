//! One client's most recent reconciliation state.

use fabric_client_model::ClientRevision;

use crate::status::ReconciliationStatus;

/// What is known about a client's reconciliation, as of the last thing that
/// happened to it.
///
/// # Why the revision is here
///
/// Because "reconciled" is only meaningful with respect to a *version* of the
/// desired state. A report saying `Applied` against revision `abc` is not a
/// claim about the document currently in Git — if an operator has since
/// written revision `def`, the honest reading is that the provider matches
/// something that is no longer what is wanted. Carrying the revision is what
/// lets the control plane and the operator see that, instead of showing a
/// green tick over stale information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationReport {
    /// Where the client stands.
    pub status: ReconciliationStatus,

    /// The desired-state revision this report is about.
    pub revision: ClientRevision,

    /// How many changes the last pass made.
    pub actions: usize,

    /// When this was recorded, in seconds since the Unix epoch.
    ///
    /// Wall-clock rather than monotonic, because it is displayed to a human
    /// and outlives the process that recorded it. Unix seconds rather than a
    /// formatted timestamp so that no date-time library — and no opinion about
    /// time zones — enters the control plane for the sake of one field.
    pub observed_at_unix: u64,

    /// Why the last pass failed, if it did.
    ///
    /// Sanitised where it was produced: [`ProviderError`](crate::ProviderError)
    /// documents that an adapter must not put an upstream response body or any
    /// credential in it, which is what makes this field safe to show an
    /// operator.
    pub detail: Option<String>,
}
