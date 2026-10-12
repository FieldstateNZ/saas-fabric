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
//! it moves to, the exact record it will write, the operator and, for a
//! restore, the exact tombstone reversed. The pending transition is frozen:
//! reserving exactly the same transition again is another attempt at the
//! same operation, so a retry or a concurrent identical request replays the
//! stored intent rather than a new one, and any other transition while one
//! is pending is refused. Every attempt has its own token, never reused. A
//! completion by any attempt, or by the operation, completes the operation.
//! A release names one attempt whose own write conclusively did not land,
//! and clears the operation only when no other attempt at it is still
//! outstanding: an attempt that wrote, or may have, keeps it pending
//! (review 5489623418).
//! A completion pairs with its own pending reservation only. A pending
//! reservation survives a failed document write that may have landed, a
//! failed completion and a restart. Storage, threat boundary, crash
//! consistency, enrollment and backup are O5 slice B's to state.
//!
//! In the 121-150 band: one port and the types its four calls exchange.

use async_trait::async_trait;
use fabric_client_model::lifecycle::{IdentityInstance, Tombstone};
use fabric_client_model::{ClientId, ClientRevision, LifecycleRecord};

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

    /// Reserves an attempt at `transition`, before the document is written:
    /// a new operation, or another attempt at the identical pending one.
    ///
    /// # Errors
    ///
    /// [`ContinuityRefusal`] when the transition does not follow what the
    /// store accepted, another is pending, or the store cannot answer.
    /// Nothing may be written after a refusal.
    async fn reserve(&self, transition: &Transition<'_>) -> Result<ContinuityReservation, ContinuityRefusal>;

    /// Records that the document write of `reservation`, an attempt or the
    /// operation it belongs to, produced `revision`.
    ///
    /// # Errors
    ///
    /// [`ContinuityRefusal::StaleCompletion`] when `reservation` is not the
    /// pending operation, or [`ContinuityRefusal::Unavailable`]. The
    /// reservation then stays pending.
    async fn complete(
        &self,
        reservation: &ContinuityReservation,
        revision: &ClientRevision,
    ) -> Result<(), ContinuityRefusal>;

    /// Releases the attempt `reservation`, whose own document write
    /// conclusively did not land. The operation stays pending while any other
    /// attempt at it is outstanding; a token of an earlier operation, or one
    /// already released or completed, changes nothing.
    async fn release(&self, reservation: &ContinuityReservation);
}

/// What the store holds for one client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedLifecycle {
    /// The identity instance the store attested at creation or enrollment.
    pub instance: IdentityInstance,
    /// How many transitions the store has completed for this instance.
    pub sequence: u64,
    /// The lifecycle, instance and identity context last confirmed.
    pub fingerprint: LifecycleFingerprint,
    /// A reserved transition not yet completed, if any.
    pub pending: Option<PendingTransition>,
}

/// A reserved transition the store has not seen completed, frozen exactly
/// as it was reserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTransition {
    /// The operation; each attempt at it has a token of its own.
    pub reservation: ContinuityReservation,
    /// Which write it is.
    pub kind: TransitionKind,
    /// The desired revision it was reviewed at; `None` for a creation.
    pub at_revision: Option<ClientRevision>,
    /// The fingerprint it records.
    pub to: LifecycleFingerprint,
    /// The exact lifecycle record it writes, so a retry replays it.
    pub to_record: LifecycleRecord,
    /// For a restore, the tombstone it reverses.
    pub reversing: Option<Tombstone>,
    /// The operator subject that reserved it.
    pub operator: String,
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
    /// The identity instance it moves, or for a creation the one minted for it.
    pub instance: &'a IdentityInstance,
    /// The sequence the store must still be at; `None` only for a creation.
    pub expected_sequence: Option<u64>,
    /// The desired revision the operator reviewed; `None` for a creation.
    pub at_revision: Option<&'a ClientRevision>,
    /// The fingerprint it moves from; `None` only for a creation.
    pub from: Option<&'a LifecycleFingerprint>,
    /// The fingerprint it records.
    pub to: &'a LifecycleFingerprint,
    /// The exact lifecycle record it writes.
    pub to_record: &'a LifecycleRecord,
    /// For a restore, the exact tombstone it reverses.
    pub reversing: Option<&'a Tombstone>,
    /// The operator subject.
    pub operator: &'a str,
}
