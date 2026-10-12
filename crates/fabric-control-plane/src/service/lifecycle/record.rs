//! Running one lifecycle write through the continuity store: reserve, write,
//! complete, and replay or recover a transition the store holds pending.
//!
//! # The partial-success and retry contract
//!
//! - A refused reservation writes nothing.
//! - A document write that lost its revision conclusively did not land, so
//!   it releases its own attempt and nothing else. An identical concurrent
//!   attempt that wrote, or may have, keeps the operation pending.
//! - Any other failed write may have landed: its reservation stays pending,
//!   and the client is held.
//! - A failed completion leaves the written transition pending and the
//!   client held; the response reports the hold, never a confirmed
//!   transition.
//! - Only the same request sent again, with the same `If-Match`, goes on
//!   (`replay`): the frozen transition is replayed exactly, or, when the
//!   document carries it, its preconditions are checked again before it is
//!   completed. Any other intent is refused as pending.
//!
//! In the 121-150 band: the write and its retry contract are read together.

use fabric_client_model::{ClientDocument, ClientId, ClientRevision, LifecycleRecord, LifecycleRefusal};

use super::replay::same_intent;
use super::write::Prepared;
use crate::continuity::{self, Reserving};
use crate::{
    logging, ClientService, ContinuityHold, ControlPlaneError, LifecycleContinuity, Operator, StoredClient,
    TransitionKind,
};

/// Whether the store confirmed a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Confirmed {
    /// No store: the document alone records it.
    NoStore,
    /// The store completed the reservation.
    Yes,
    /// The document is written and the reservation is still pending.
    Pending,
}

/// One lifecycle write, and where it is reserved.
pub(super) struct Asked<'a> {
    pub(super) operator: &'a Operator,
    pub(super) id: &'a ClientId,
    pub(super) expected: &'a ClientRevision,
    pub(super) kind: TransitionKind,
    pub(super) action: &'static str,
    /// The reason the request gives, for a write that takes one.
    pub(super) reason: Option<&'a str>,
    pub(super) continuity: Option<&'a dyn LifecycleContinuity>,
}

/// What a write did.
pub(super) enum Ran {
    /// It wrote the document.
    Written(StoredClient, Confirmed),
    /// It completed a pending transition the document already carried.
    Recovered(StoredClient),
}

impl ClientService {
    /// Runs `transition` as `asked`, checking `precondition` against the
    /// stored document before anything is reserved, written or completed.
    pub(super) async fn run_transition(
        &self,
        asked: &Asked<'_>,
        transition: impl Fn(&LifecycleRecord) -> Result<LifecycleRecord, LifecycleRefusal> + Sync,
        precondition: impl Fn(&ClientDocument) -> Result<(), ControlPlaneError> + Sync,
    ) -> Result<Ran, ControlPlaneError> {
        let pending = self.pending(asked).await?;
        let prepared = match &pending {
            Some(frozen) => {
                let current = self.get(asked.id).await?;
                if current.revision != *asked.expected {
                    return self.recover(asked, frozen, current, &precondition).await;
                }
                let fresh = self.prepare(asked, &transition).await?;
                if !same_intent(asked.kind, &fresh.to, &frozen.to_record) {
                    return Err(ContinuityHold::Pending.into());
                }
                self.prepare(asked, &|_: &LifecycleRecord| Ok(frozen.to_record.clone()))
                    .await?
            }
            None => self.prepare(asked, &transition).await?,
        };
        precondition(&prepared.current)?;
        let (stored, confirmed) = self.record_transition(asked, prepared).await?;
        Ok(Ran::Written(stored, confirmed))
    }

    async fn prepare(
        &self,
        asked: &Asked<'_>,
        transition: &(dyn Fn(&LifecycleRecord) -> Result<LifecycleRecord, LifecycleRefusal> + Sync),
    ) -> Result<Prepared, ControlPlaneError> {
        self.prepare_lifecycle(asked.operator, asked.id, asked.expected, asked.action, transition)
            .await
    }

    /// Writes `prepared`, reserving it first when there is a store.
    async fn record_transition(
        &self,
        asked: &Asked<'_>,
        prepared: Prepared,
    ) -> Result<(StoredClient, Confirmed), ControlPlaneError> {
        let (operator, id, expected) = (asked.operator, asked.id, asked.expected);
        let Some(continuity) = asked.continuity else {
            let stored = self.write_lifecycle(operator, id, expected, prepared).await?;
            return Ok((stored, Confirmed::NoStore));
        };
        let reversing = (asked.kind == TransitionKind::Restore)
            .then(|| prepared.from.tombstone.clone())
            .flatten();
        let reservation = continuity::reserve(
            continuity,
            &Reserving {
                client: id,
                realm: &prepared.current.client().identity.realm,
                kind: asked.kind,
                at_revision: Some(expected),
                from: Some(&prepared.from),
                to: &prepared.to,
                reversing: reversing.as_ref(),
                operator: operator.subject(),
            },
        )
        .await?;

        let stored = match self.write_lifecycle(operator, id, expected, prepared).await {
            Ok(stored) => stored,
            Err(ControlPlaneError::RevisionConflict) => {
                continuity.release(&reservation).await;
                return Err(ControlPlaneError::RevisionConflict);
            }
            Err(error) => {
                logging::continuity_write_unconfirmed(id, reservation.id());
                return Err(error);
            }
        };
        match continuity.complete(&reservation, &stored.revision).await {
            Ok(()) => Ok((stored, Confirmed::Yes)),
            Err(refusal) => {
                logging::continuity_completion_failed(id, reservation.id(), &refusal);
                Ok((stored, Confirmed::Pending))
            }
        }
    }
}
