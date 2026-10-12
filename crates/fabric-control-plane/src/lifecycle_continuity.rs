//! Where an operator's restoration of a tombstoned client is reserved before
//! the document records it (D01-14 restore; O5, #108 comment 6119161317).
//!
//! # Partial: no durable adapter exists
//!
//! A tombstone cannot be undone by Git, and a supported reinstatement is a
//! new forward transition recorded in protected continuity state: durable,
//! independent of editable desired-state bytes, with revision, check-and-set
//! and reservation semantics. Without that, a restoration written by this
//! control plane and one written into Git by hand read the same. The store
//! does not exist yet, so a deployment is built with no
//! [`LifecycleContinuity`], and every restore is held before anything is
//! written (`503 lifecycle_continuity_unavailable`).
//!
//! Tests hand in `testing::InMemoryLifecycleContinuity` to prove the order
//! of the write. It is process-only and certifies nothing about restarts,
//! crashes or concurrent writers.
//!
//! # What an adapter must do before it is enabled
//!
//! Answer only from provenance it can vouch is current for the client's
//! identity instance: unenrolled, stale or unavailable history refuses. A
//! reservation is durable before the document write; a completion pairs
//! with its own reservation and is rejected when stale; a reservation whose
//! write never happened is released, and one whose completion failed stays
//! pending across a restart. Its reviewed enrollment procedure, storage and
//! threat boundary, crash consistency and backup contract belong to O5
//! slice B, which shares this store with D01-15's declaration ledger.

use async_trait::async_trait;
use fabric_client_model::{ClientId, ClientRevision, LifecycleRecord};

/// The protected continuity state for client lifecycles.
#[async_trait]
pub trait LifecycleContinuity: Send + Sync {
    /// Reserves `restoration` as a forward transition, before the document
    /// is written.
    ///
    /// # Errors
    ///
    /// [`ContinuityRefusal`] when the store has no history it can vouch for
    /// this client, when its history does not end at `restoration.from`, or
    /// when it cannot be reached. Nothing may be written after a refusal.
    async fn reserve(&self, restoration: &Restoring<'_>) -> Result<ContinuityReservation, ContinuityRefusal>;

    /// Records that the document write for `reservation` produced `revision`.
    ///
    /// # Errors
    ///
    /// [`ContinuityRefusal`] when the completion could not be recorded. The
    /// document is written by then; the reservation stays pending.
    async fn complete(
        &self,
        reservation: &ContinuityReservation,
        revision: &ClientRevision,
    ) -> Result<(), ContinuityRefusal>;

    /// Releases a reservation whose document write did not happen.
    async fn release(&self, reservation: &ContinuityReservation);
}

/// One restoration, as the store is asked to reserve it.
#[derive(Debug)]
pub struct Restoring<'a> {
    /// The client.
    pub client: &'a ClientId,
    /// The document revision the restoration was read at.
    pub at_revision: &'a ClientRevision,
    /// The lifecycle the document records now, ending in the tombstone being
    /// reversed.
    pub from: &'a LifecycleRecord,
    /// The lifecycle the write will record.
    pub to: &'a LifecycleRecord,
    /// The operator subject restoring it.
    pub operator: &'a str,
}

/// A reservation the store handed out. Opaque to the control plane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuityReservation(String);

impl ContinuityReservation {
    /// A reservation the store names `id`.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The store's name for it.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.0
    }
}

/// Why the store would not reserve or complete a transition.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContinuityRefusal {
    /// The store holds no history it can vouch for this client: it was
    /// never enrolled, or its enrollment is stale.
    #[error("the continuity store holds no accepted lifecycle for this client")]
    Unenrolled,
    /// The store's accepted lifecycle does not end where the document's does:
    /// the document moved outside a supported transition.
    #[error("the client's recorded lifecycle does not match the continuity store's")]
    Mismatch,
    /// The store could not be reached or did not answer.
    #[error("the continuity store is unavailable: {detail}")]
    Unavailable {
        /// What failed, for the log; never shown to a browser.
        detail: String,
    },
}
