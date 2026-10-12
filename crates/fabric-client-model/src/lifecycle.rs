//! A client's lifecycle: saved intent, ready, tombstoned, deletion confirmed
//! (D01-14, D01-19a).
//!
//! ```text
//!  create ──► incomplete ◄──evidence──► ready
//!                 │                       │
//!                 └──── tombstone ────────┘
//!                           ▼        ▲
//!                      tombstoned ───┘ restore: back to incomplete, the
//!                           │          tombstone kept in `restorations`
//!                           │ confirm deletion (a separate write)
//!                           ▼
//!                   deletionConfirmed  (intent only; nothing is deleted)
//! ```
//!
//! Two of these states are recorded in desired state ([`LifecycleRecord`]):
//! an operator wrote them. The other two are derived from evidence on every
//! read ([`Readiness`]), because "usable" is a fact about the platform rather
//! than something an operator declares. A restored client is saved intent
//! again: it is ready only when the evidence says so at its new revision.
//! There is no transition out of deletion confirmed: what it eventually
//! removes, and whether it can ever be restored, are not decided.

mod entries;
mod instance;
mod readiness;
mod record;
mod refusal;
mod state;

#[cfg(test)]
mod lifecycle_tests;

pub use entries::{DeletionConfirmation, Restoration, Tombstone};
pub use instance::IdentityInstance;
pub use readiness::{Evidence, Readiness, ReadinessCondition, Unmet};
pub use record::LifecycleRecord;
pub use refusal::{LifecycleRefusal, LifecycleUse};
pub use state::LifecycleState;

/// Where a client stands, with what each state carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientLifecycle {
    /// Saved intent, with the conditions that are not met yet.
    Incomplete {
        /// Never empty.
        unmet: Vec<Unmet>,
    },
    /// A usable client.
    Ready,
    /// Taken out of service.
    Tombstoned {
        /// The recorded tombstone.
        tombstone: Tombstone,
    },
    /// Deletion confirmed.
    DeletionConfirmed {
        /// The recorded tombstone.
        tombstone: Tombstone,
        /// The recorded confirmation.
        deletion: DeletionConfirmation,
    },
}

impl ClientLifecycle {
    /// Derives the state from what is recorded and what the evidence shows.
    ///
    /// A recorded tombstone wins over any evidence: a retired client that is
    /// still provisioned, applied and published is not ready.
    #[must_use]
    pub fn derive(record: &LifecycleRecord, readiness: Readiness) -> Self {
        match (&record.tombstone, &record.deletion_confirmed) {
            (Some(tombstone), Some(deletion)) => Self::DeletionConfirmed {
                tombstone: tombstone.clone(),
                deletion: deletion.clone(),
            },
            (Some(tombstone), None) => Self::Tombstoned {
                tombstone: tombstone.clone(),
            },
            // A document that parsed cannot hold a confirmation without a
            // tombstone (`LifecycleRecord::check`), so this arm is active.
            (None, _) => {
                let unmet = readiness.into_unmet();
                if unmet.is_empty() {
                    Self::Ready
                } else {
                    Self::Incomplete { unmet }
                }
            }
        }
    }

    /// Which state this is.
    #[must_use]
    pub const fn state(&self) -> LifecycleState {
        match self {
            Self::Incomplete { .. } => LifecycleState::Incomplete,
            Self::Ready => LifecycleState::Ready,
            Self::Tombstoned { .. } => LifecycleState::Tombstoned,
            Self::DeletionConfirmed { .. } => LifecycleState::DeletionConfirmed,
        }
    }

    /// Whether this state permits `operation`.
    ///
    /// A retired client permits none of these. Saved intent permits all but
    /// routing: editing and placement are steps towards readiness.
    ///
    /// Publication is not a use this governs. Saved intent must be
    /// publishable, because publication is one of its readiness conditions;
    /// a retired client's tenant stays published until a separately
    /// reviewed withdrawal exists, because recording retirement is intent
    /// only (ADR 0021's 2026-10-11 amendment, "Withdrawal is deferred").
    ///
    /// Identity edits and secret writes are not uses this governs. They are
    /// how an operator removes a compromised redirect URI or rotates a leaked
    /// secret, and a tombstone does not revoke the client's identity.
    ///
    /// # Errors
    ///
    /// [`LifecycleRefusal::Retired`] or [`LifecycleRefusal::NotReady`].
    pub fn permits(&self, operation: LifecycleUse) -> Result<(), LifecycleRefusal> {
        match (self, operation) {
            (Self::Tombstoned { .. } | Self::DeletionConfirmed { .. }, _) => Err(LifecycleRefusal::Retired {
                state: self.state(),
                operation,
            }),
            (Self::Incomplete { unmet }, LifecycleUse::Route) => Err(LifecycleRefusal::NotReady {
                operation,
                unmet: unmet.iter().map(|unmet| unmet.condition).collect(),
            }),
            (Self::Incomplete { .. } | Self::Ready, _) => Ok(()),
        }
    }
}
