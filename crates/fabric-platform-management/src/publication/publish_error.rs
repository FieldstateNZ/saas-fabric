//! Sorting what a [`RuntimePublication`](fabric_runtime_publication::RuntimePublication)
//! port refused into what an operator can do about it.

use fabric_runtime_publication::PublicationError;

use crate::publication::outcome::PassOutcome;
use crate::SafeDiagnostic;

/// Sorts what the target refused into what an operator can do about it.
///
/// [`PublicationError::Unwritable`] is its own case, not folded into
/// `Refused` with the rest: it is the one variant that may have written
/// something before failing (the cluster, or the disk, can be
/// half-updated -- see [`PublicationError`]'s own rustdoc), so it is a
/// transport failure to retry, the same as [`PublicationError::Unreadable`]
/// reaching a held document -- which is also every error
/// [`RuntimePublication::current`](fabric_runtime_publication::RuntimePublication::current)
/// can return, so `run_pass` routes that call through this same function
/// rather than assuming `Failed` on its behalf. [`PublicationError::StaleRevision`]
/// joins them: the next pass reads the revision that moved and offers
/// against it, so it self-heals the way a transport failure does, not the
/// way a coherence problem does. Everything else here names a document this
/// platform itself (or a hand edit) produced in a shape the target will
/// never accept unchanged -- including a [`PublicationError::DivergentPayload`]
/// that survived every retry, which means something is still rewriting a
/// document out from under this pass, not that the pass mis-offered it once.
pub(super) fn outcome_from_publish_error(error: &PublicationError) -> PassOutcome {
    match error {
        PublicationError::Unwritable { .. }
        | PublicationError::Unreadable { .. }
        | PublicationError::StaleRevision { .. } => PassOutcome::Failed {
            detail: SafeDiagnostic::sanitise(&error.to_string()),
        },
        PublicationError::DivergentPayload { .. }
        | PublicationError::DanglingDataSource { .. }
        | PublicationError::RetiredDataSourceStillBound { .. }
        | PublicationError::EmptyingNotIntended { .. }
        | PublicationError::EmptyCatalogue
        | PublicationError::EmptyTenantData { .. }
        | PublicationError::HeldPayloadLost { .. } => PassOutcome::Refused {
            reason: SafeDiagnostic::sanitise(&error.to_string()),
        },
    }
}
