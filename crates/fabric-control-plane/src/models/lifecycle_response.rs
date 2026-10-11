//! What a client's lifecycle looks like on the wire.

use fabric_client_model::lifecycle::{DeletionConfirmation, Tombstone, Unmet};
use fabric_client_model::{ClientLifecycle, LifecycleState};

/// Where a client stands: saved intent, ready, tombstoned or deletion
/// confirmed (D01-14, D01-19a).
///
/// `unmet` is empty unless the state is `incomplete`; `tombstone` and
/// `deletionConfirmed` are null until recorded. A console tells saved intent
/// from a usable client by `state` alone.
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
}

impl From<ClientLifecycle> for LifecycleResponse {
    fn from(lifecycle: ClientLifecycle) -> Self {
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
        }
    }
}
