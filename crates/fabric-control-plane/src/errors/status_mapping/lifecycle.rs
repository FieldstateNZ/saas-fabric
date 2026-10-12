//! Which status, and which stable code, a lifecycle refusal carries.
//!
//! | Code | Status | When |
//! |---|---|---|
//! | `client_retired` | 409 | the client is tombstoned or its deletion is confirmed, and the request would change or serve it |
//! | `client_not_ready` | 409 | the client is saved intent, and the request needs a usable client |
//! | `client_already_tombstoned` | 409 | a second tombstone |
//! | `deletion_requires_tombstone` | 409 | deletion confirmed for a client that was never tombstoned |
//! | `deletion_already_confirmed` | 409 | a second confirmation, or a tombstone after one |
//! | `deletion_confirmation_mismatch` | 422 | the confirmation names a different client |
//! | `invalid_tombstone_reason` | 422 | a blank, oversized or control-character reason |
//! | `client_not_tombstoned` | 409 | a restore of a client that is not tombstoned |
//! | `deletion_confirmed_not_restorable` | 409 | a restore once deletion is confirmed |
//! | `invalid_restoration_reason` | 422 | a blank, oversized or control-character restoration reason |

use fabric_client_model::LifecycleRefusal;
use http::StatusCode;

/// The status an operator's browser sees.
///
/// 409 for every refusal about the client's state: the request was well
/// formed and is refused because of where the client stands, which only a
/// different lifecycle operation changes. 422 for those that are wrong in
/// themselves, whatever state the client is in.
pub(super) const fn status(refusal: &LifecycleRefusal) -> StatusCode {
    match refusal {
        LifecycleRefusal::Retired { .. }
        | LifecycleRefusal::NotReady { .. }
        | LifecycleRefusal::AlreadyTombstoned
        | LifecycleRefusal::DeletionRequiresTombstone
        | LifecycleRefusal::DeletionAlreadyConfirmed
        | LifecycleRefusal::RestoreRequiresTombstone
        | LifecycleRefusal::DeletionConfirmedNotRestorable => StatusCode::CONFLICT,
        LifecycleRefusal::ConfirmationMismatch
        | LifecycleRefusal::InvalidReason { .. }
        | LifecycleRefusal::InvalidRestorationReason { .. } => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

/// A stable machine-readable code.
pub(super) const fn code(refusal: &LifecycleRefusal) -> &'static str {
    match refusal {
        LifecycleRefusal::Retired { .. } => "client_retired",
        LifecycleRefusal::NotReady { .. } => "client_not_ready",
        LifecycleRefusal::AlreadyTombstoned => "client_already_tombstoned",
        LifecycleRefusal::DeletionRequiresTombstone => "deletion_requires_tombstone",
        LifecycleRefusal::DeletionAlreadyConfirmed => "deletion_already_confirmed",
        LifecycleRefusal::ConfirmationMismatch => "deletion_confirmation_mismatch",
        LifecycleRefusal::InvalidReason { .. } => "invalid_tombstone_reason",
        LifecycleRefusal::RestoreRequiresTombstone => "client_not_tombstoned",
        LifecycleRefusal::DeletionConfirmedNotRestorable => "deletion_confirmed_not_restorable",
        LifecycleRefusal::InvalidRestorationReason { .. } => "invalid_restoration_reason",
    }
}
