//! The continuity store's port: what it accepted, and reserving and
//! completing one lifecycle transition at a time.
//!
//! # What an adapter must do before it is enabled
//!
//! Answer only from history it can vouch for: unenrolled, stale or
//! unavailable history refuses, and nothing is enrolled from desired-state
//! files or provider attributes. A reservation is durable before the
//! document write and binds the identity instance, the expected sequence,
//! the reviewed desired revision, the fingerprint it moves from and the one
//! it moves to, and, for a restore, the exact tombstone reversed. Reserving
//! the same transition again (same kind, instance, sequence, reviewed
//! revision and starting lifecycle) returns the pending reservation with its
//! new destination, so a retry whose first write never landed recovers it:
//! the document could not have moved while it is still at that revision. Any
//! other transition while one is pending is refused.
//! A completion pairs with its own pending reservation only. A pending
//! reservation survives a failed document write that may have landed, a
//! failed completion and a restart. Storage, threat boundary, crash
//! consistency, enrollment and backup are O5 slice B's to state.

use async_trait::async_trait;
use fabric_client_model::lifecycle::Tombstone;
use fabric_client_model::{ClientId, ClientRevision};

use super::{ContinuityRefusal, ContinuityReservation, LifecycleFingerprint};

/// The protected continuity state for client lifecycles.
#[async_trait]
pub trait LifecycleContinuity: Send + Sync {
    /// What the store accepted for `client`; `None` when it holds nothing.
    ///
    /// # Errors
    ///
    /// [`ContinuityRefusal::Unavailable`] when the store cannot answer.
    async fn accepted(&self, client: &ClientId) -> Result<Option<AcceptedLifecycle>, ContinuityRefusal>;

    /// Reserves `transition`, before the document is written.
    ///
    /// # Errors
    ///
    /// [`ContinuityRefusal`] when the transition does not follow what the
    /// store accepted, another is pending, or the store cannot answer.
    /// Nothing may be written after a refusal.
    async fn reserve(&self, transition: &Transition<'_>) -> Result<ContinuityReservation, ContinuityRefusal>;

    /// Records that the document write for `reservation` produced `revision`.
    ///
    /// # Errors
    ///
    /// [`ContinuityRefusal::StaleCompletion`] when `reservation` is not the
    /// pending one, or [`ContinuityRefusal::Unavailable`]. The reservation
    /// then stays pending.
    async fn complete(
        &self,
        reservation: &ContinuityReservation,
        revision: &ClientRevision,
    ) -> Result<(), ContinuityRefusal>;

    /// Releases `reservation`, whose document write is known not to have
    /// happened.
    async fn release(&self, reservation: &ContinuityReservation);
}

/// What the store holds for one client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedLifecycle {
    /// The client's identity instance: a client id created again after its
    /// document was removed is a different instance, never this one.
    pub instance: u64,
    /// How many transitions the store has completed for this instance.
    pub sequence: u64,
    /// The lifecycle the last completed transition recorded.
    pub fingerprint: LifecycleFingerprint,
    /// A reserved transition not yet completed, if any.
    pub pending: Option<PendingTransition>,
}

/// A reserved transition the store has not seen completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTransition {
    /// The reservation.
    pub reservation: ContinuityReservation,
    /// The lifecycle it records.
    pub to: LifecycleFingerprint,
    /// The desired revision it was reviewed at; `None` for a creation.
    pub at_revision: Option<ClientRevision>,
    /// Which write it is.
    pub kind: TransitionKind,
}

/// Which lifecycle write a transition is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    /// A new identity instance, never a resurrected one.
    Create,
    /// A tombstone.
    Tombstone,
    /// A deletion confirmation.
    ConfirmDeletion,
    /// A restoration reversing a tombstone.
    Restore,
}

/// One lifecycle transition, as the store is asked to reserve it.
#[derive(Debug)]
pub struct Transition<'a> {
    /// The client.
    pub client: &'a ClientId,
    /// Which write.
    pub kind: TransitionKind,
    /// The identity instance it moves; `None` only for a creation.
    pub instance: Option<u64>,
    /// The sequence the store must still be at; `None` only for a creation.
    pub expected_sequence: Option<u64>,
    /// The desired revision the operator reviewed; `None` for a creation.
    pub at_revision: Option<&'a ClientRevision>,
    /// The lifecycle it moves from; `None` only for a creation.
    pub from: Option<&'a LifecycleFingerprint>,
    /// The lifecycle it records.
    pub to: &'a LifecycleFingerprint,
    /// For a restore, the exact tombstone it reverses.
    pub reversing: Option<&'a Tombstone>,
    /// The operator subject.
    pub operator: &'a str,
}
