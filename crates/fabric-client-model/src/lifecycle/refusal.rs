//! What the lifecycle refuses, and what a refusal is about.

use super::readiness::ReadinessCondition;
use super::LifecycleState;

/// Something done to or with a client that its lifecycle can refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LifecycleUse {
    /// Changing the client's product configuration: its name, hosts and
    /// application entitlements.
    Edit,
    /// Removing something the client holds, such as a secret version.
    Remove,
    /// Recording a data placement for the client.
    Place,
    /// Serving the client's end users as a usable client.
    Route,
}

impl std::fmt::Display for LifecycleUse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Edit => "editing",
            Self::Remove => "removal",
            Self::Place => "placement",
            Self::Route => "routing",
        })
    }
}

/// Why a lifecycle transition or use was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleRefusal {
    /// The client is tombstoned or its deletion is confirmed, and its state is
    /// retained as it was.
    #[error("the client is {state}, so {operation} is refused; its state is retained")]
    Retired {
        /// Which retired state it is in.
        state: LifecycleState,
        /// What was refused.
        operation: LifecycleUse,
    },

    /// The client is saved intent, not yet a usable client.
    #[error("the client is incomplete, so {operation} is refused until it is ready")]
    NotReady {
        /// What was refused.
        operation: LifecycleUse,
        /// Which readiness conditions are not met.
        unmet: Vec<ReadinessCondition>,
    },

    /// The client is already tombstoned.
    #[error("the client is already tombstoned")]
    AlreadyTombstoned,

    /// Deletion can only be confirmed for a tombstoned client.
    #[error("deletion can only be confirmed for a tombstoned client")]
    DeletionRequiresTombstone,

    /// Deletion is already confirmed, which is the last state.
    #[error("deletion of the client is already confirmed")]
    DeletionAlreadyConfirmed,

    /// The confirmation named a different client.
    #[error("the confirmation does not name this client")]
    ConfirmationMismatch,

    /// The tombstone reason is not acceptable.
    #[error("{detail}")]
    InvalidReason {
        /// What is wrong with it.
        detail: &'static str,
    },

    /// Only a tombstoned client can be restored.
    #[error("only a tombstoned client can be restored")]
    RestoreRequiresTombstone,

    /// Deletion is confirmed, and a confirmed deletion is not restored: it
    /// needs an explicit disposition first (ADR 0021, "Restoring a
    /// tombstone"), and none exists.
    #[error("deletion of the client is confirmed, so it cannot be restored")]
    DeletionConfirmedNotRestorable,

    /// The restoration reason is not acceptable.
    #[error("{detail}")]
    InvalidRestorationReason {
        /// What is wrong with it.
        detail: &'static str,
    },
}
