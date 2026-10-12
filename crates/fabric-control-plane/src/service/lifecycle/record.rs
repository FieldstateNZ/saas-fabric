//! Writing a prepared lifecycle transition through the continuity store:
//! reserve, write, complete, and recover a transition whose completion was
//! not recorded.
//!
//! # The partial-success and retry contract
//!
//! - A refused reservation writes nothing.
//! - A document write that lost its revision released the reservation:
//!   nothing was written.
//! - Any other failed write may have landed, so the reservation stays
//!   pending, and the client is held.
//! - A failed completion leaves the written transition pending and the
//!   client held; the response reports the hold, never a confirmed
//!   transition.
//! - Sending the same request again, with the same `If-Match`, recovers
//!   either: if the document carries exactly the pending transition, the
//!   reservation is completed at the document's revision; if the document
//!   is still at the reviewed revision, the store hands back the same
//!   reservation and the write is made.

use fabric_client_model::{ClientId, ClientRevision};

use super::write::Prepared;
use crate::continuity::{self, Reserving};
use crate::{
    logging, ClientService, ContinuityHold, ControlPlaneError, LifecycleContinuity, LifecycleFingerprint,
    Operator, StoredClient, TransitionKind,
};

/// Whether the store confirmed a written transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Confirmed {
    /// No store: the document alone records it.
    NoStore,
    /// The store completed the reservation.
    Yes,
    /// The document is written and the reservation is still pending.
    Pending,
}

impl ClientService {
    /// Writes `prepared`, reserving it in `continuity` when there is one.
    pub(super) async fn record_transition(
        &self,
        operator: &Operator,
        id: &ClientId,
        expected: &ClientRevision,
        prepared: Prepared,
        continuity: Option<&dyn LifecycleContinuity>,
        kind: TransitionKind,
    ) -> Result<(StoredClient, Confirmed), ControlPlaneError> {
        let Some(continuity) = continuity else {
            let stored = self.write_lifecycle(operator, id, expected, prepared).await?;
            return Ok((stored, Confirmed::NoStore));
        };
        let reversing = (kind == TransitionKind::Restore)
            .then(|| prepared.from.tombstone.clone())
            .flatten();
        let reservation = continuity::reserve(
            continuity,
            &Reserving {
                client: id,
                kind,
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

    /// The client as stored, when a `kind` transition reviewed at `expected`
    /// is pending and the document already carries exactly it: its
    /// reservation is then completed at the document's revision.
    pub(super) async fn recover_transition(
        &self,
        id: &ClientId,
        expected: &ClientRevision,
        continuity: Option<&dyn LifecycleContinuity>,
        kind: TransitionKind,
    ) -> Result<Option<StoredClient>, ControlPlaneError> {
        let Some(continuity) = continuity else {
            return Ok(None);
        };
        let pending = match continuity.accepted(id).await {
            Ok(accepted) => accepted.and_then(|accepted| accepted.pending),
            Err(refusal) => return Err(ContinuityHold::from_refusal(&refusal).into()),
        };
        let Some(pending) = pending.filter(|pending| pending.kind == kind) else {
            return Ok(None);
        };
        if pending.at_revision.as_ref() != Some(expected) {
            return Ok(None);
        }
        let current = self.get(id).await?;
        if current.revision == *expected
            || pending.to != LifecycleFingerprint::of(current.document.lifecycle())
        {
            return Ok(None);
        }
        continuity
            .complete(&pending.reservation, &current.revision)
            .await
            .map_err(|refusal| ControlPlaneError::from(ContinuityHold::from_refusal(&refusal)))?;
        Ok(Some(current))
    }
}
