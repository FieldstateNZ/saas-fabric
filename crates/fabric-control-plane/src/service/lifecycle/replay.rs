//! Finding the store's pending transition for a retried request, and
//! recovering one the document already carries (see `record` for the
//! contract).

use fabric_client_model::{ClientDocument, LifecycleRecord};

use super::record::{Asked, Ran};
use crate::{
    ClientService, ContinuityHold, ControlPlaneError, LifecycleFingerprint, PendingTransition, StoredClient,
    TransitionKind,
};

impl ClientService {
    /// The store's pending transition of this kind, reviewed at this
    /// revision, if there is one.
    pub(super) async fn pending(
        &self,
        asked: &Asked<'_>,
    ) -> Result<Option<PendingTransition>, ControlPlaneError> {
        let Some(continuity) = asked.continuity else {
            return Ok(None);
        };
        let accepted = continuity
            .accepted(asked.id)
            .await
            .map_err(|refusal| ControlPlaneError::from(ContinuityHold::from_refusal(&refusal)))?;
        Ok(accepted.and_then(|accepted| accepted.pending).filter(|pending| {
            pending.kind == asked.kind && pending.at_revision.as_ref() == Some(asked.expected)
        }))
    }

    /// Completes `frozen` when the document carries exactly its lifecycle
    /// and `precondition` still holds; otherwise leaves it pending.
    pub(super) async fn recover(
        &self,
        asked: &Asked<'_>,
        frozen: &PendingTransition,
        current: StoredClient,
        precondition: &(dyn Fn(&ClientDocument) -> Result<(), ControlPlaneError> + Sync),
    ) -> Result<Ran, ControlPlaneError> {
        if frozen.to != LifecycleFingerprint::of_document(&current.document) {
            return Err(ContinuityHold::Pending.into());
        }
        precondition(&current.document)?;
        let Some(continuity) = asked.continuity else {
            return Err(ContinuityHold::NoStore.into());
        };
        continuity
            .complete(&frozen.reservation, &current.revision)
            .await
            .map_err(|refusal| ControlPlaneError::from(ContinuityHold::from_refusal(&refusal)))?;
        Ok(Ran::Recovered(current))
    }
}

/// Whether `fresh` asks for the same write as the frozen `pending` record:
/// equal in everything but the second the new entry was stamped with.
pub(super) fn same_intent(kind: TransitionKind, fresh: &LifecycleRecord, pending: &LifecycleRecord) -> bool {
    let unstamped = |record: &LifecycleRecord| {
        let mut record = record.clone();
        match kind {
            TransitionKind::Tombstone => record.tombstone.iter_mut().for_each(|entry| entry.at = 0),
            TransitionKind::ConfirmDeletion => {
                record
                    .deletion_confirmed
                    .iter_mut()
                    .for_each(|entry| entry.at = 0);
            }
            TransitionKind::Restore => record
                .restorations
                .last_mut()
                .into_iter()
                .for_each(|entry| entry.at = 0),
            TransitionKind::Create => {}
        }
        record
    };
    unstamped(fresh) == unstamped(pending)
}
