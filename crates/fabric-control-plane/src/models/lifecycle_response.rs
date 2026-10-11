//! What a client's lifecycle looks like on the wire.

use fabric_client_model::lifecycle::{DeletionConfirmation, Tombstone, Unmet};
use fabric_client_model::{ClientLifecycle, LifecycleState};

use crate::lifecycle::Withdrawal;

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
}

impl LifecycleResponse {
    /// Renders `lifecycle`, with the withdrawal evidence for a retired one.
    pub(crate) fn new(lifecycle: ClientLifecycle, withdrawal: Option<Withdrawal>) -> Self {
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
        }
    }
}
