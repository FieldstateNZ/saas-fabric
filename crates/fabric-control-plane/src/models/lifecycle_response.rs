//! What a client's lifecycle looks like on the wire.

use fabric_client_model::lifecycle::{DeletionConfirmation, Restoration, Tombstone, Unmet};
use fabric_client_model::{ClientLifecycle, LifecycleState};

use crate::continuity::Admission;
use crate::lifecycle::Withdrawal;
use crate::ContinuityHold;

/// Where a client stands: saved intent, ready, or retirement recorded
/// (D01-14, D01-19a).
///
/// `unmet` is empty unless the state is `incomplete`; `tombstone` and
/// `deletionConfirmed` are null until recorded. A console tells saved intent
/// from a usable client by `state` alone.
///
/// `withdrawal` is null unless retirement is recorded, and then reports
/// publication evidence only. A recorded tombstone is never, on its own, a
/// claim that the tenant stopped being served.
///
/// `restorations` lists every tombstone an operator reversed, oldest first,
/// in every state: a restored client shows it was once retired.
///
/// `continuity` says whether the protected continuity store confirms the
/// recorded lifecycle (O5). A held client is never identity-ready.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LifecycleResponse {
    /// Which state.
    pub(crate) state: LifecycleState,

    /// The readiness conditions not met, and why.
    pub(crate) unmet: Vec<Unmet>,

    /// The recorded tombstone.
    pub(crate) tombstone: Option<Tombstone>,

    /// The recorded deletion confirmation. Intent only: nothing is deleted.
    pub(crate) deletion_confirmed: Option<DeletionConfirmation>,

    /// Whether a retired client's tenant is still published.
    pub(crate) withdrawal: Option<Withdrawal>,

    /// Every reversed tombstone, oldest first.
    pub(crate) restorations: Vec<Restoration>,

    /// Whether the continuity store confirms the recorded lifecycle.
    pub(crate) continuity: ContinuityResponse,
}

/// Whether the continuity store confirms a client's recorded lifecycle.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContinuityResponse {
    /// `admitted` or `held`.
    pub(crate) state: &'static str,
    /// Why it is held; null when admitted.
    pub(crate) reason: Option<ContinuityHold>,
    /// The same, in an operator's words; null when admitted.
    pub(crate) detail: Option<String>,
}

impl From<Admission> for ContinuityResponse {
    fn from(admission: Admission) -> Self {
        match admission {
            Admission::Admitted => Self {
                state: "admitted",
                reason: None,
                detail: None,
            },
            Admission::Held(hold) => Self {
                state: "held",
                reason: Some(hold),
                detail: Some(hold.to_string()),
            },
        }
    }
}

impl LifecycleResponse {
    /// Renders `lifecycle`, with the withdrawal evidence for a retired one
    /// and the recorded restorations.
    pub(crate) fn new(
        lifecycle: ClientLifecycle,
        withdrawal: Option<Withdrawal>,
        restorations: Vec<Restoration>,
        admission: Admission,
    ) -> Self {
        let state = lifecycle.state();
        let (unmet, tombstone, deletion_confirmed) = match lifecycle {
            ClientLifecycle::Incomplete { unmet } => (unmet, None, None),
            ClientLifecycle::Ready => (Vec::new(), None, None),
            ClientLifecycle::Tombstoned { tombstone } => (Vec::new(), Some(tombstone), None),
            ClientLifecycle::DeletionConfirmed { tombstone, deletion } => {
                (Vec::new(), Some(tombstone), Some(deletion))
            }
        };

        Self {
            state,
            unmet,
            tombstone,
            deletion_confirmed,
            withdrawal,
            restorations,
            continuity: admission.into(),
        }
    }
}
