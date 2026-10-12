//! Restoring a tombstoned client: the explicit operator write that reverses
//! a tombstone (D01-14; ADR 0021, "Restoring a tombstone"; #108 comments
//! 6113749198 item 3 and 6119161317 item 2).
//!
//! In order, and nothing is written until every check before the write has
//! passed:
//!
//! 1. the client is read at the revision the operator names (`If-Match`);
//! 2. the lifecycle reverses its tombstone, keeping it in `restorations`, and
//!    refuses a client that is not tombstoned or whose deletion is confirmed;
//! 3. its placements must be as it left them (`lifecycle::retained`);
//! 4. the continuity store reserves the transition, and with no store the
//!    restore is held;
//! 5. the document is written at that revision, and reconciliation is marked
//!    pending at the new one;
//! 6. the reservation is completed, and the durable trail records it.
//!
//! The restore touches nothing else: no placement, tenant binding, resource
//! history, secret or identity provider. Identity comes back only when a
//! pass applies the new revision with a consistent projection.

use fabric_client_model::{ClientId, ClientRevision};

use crate::lifecycle::facts::PlatformFacts;
use crate::lifecycle::retained;
use crate::{
    audit, logging, ClientService, ContinuityRefusal, ControlPlaneError, LifecycleContinuity, Operator,
    RestoreRefusal, Restoring, StoredClient,
};

impl ClientService {
    /// Restores a tombstoned client, retaining its tombstone in history.
    ///
    /// # Errors
    ///
    /// [`ControlPlaneError::RevisionConflict`] if `expected` is stale;
    /// [`ControlPlaneError::Lifecycle`] if the client is not tombstoned, its
    /// deletion is confirmed, or the reason is not acceptable; and
    /// [`ControlPlaneError::Restore`] if its placements were not retained, or
    /// the continuity store is absent or refuses. Nothing is written after
    /// any of these.
    pub(crate) async fn restore(
        &self,
        operator: &Operator,
        id: &ClientId,
        reason: &str,
        expected: &ClientRevision,
        facts: &PlatformFacts,
        continuity: Option<&dyn LifecycleContinuity>,
    ) -> Result<StoredClient, ControlPlaneError> {
        let at = self.clock.now_unix_seconds();
        let prepared = self
            .prepare_lifecycle(operator, id, expected, "Client restored", |record| {
                record.restore(at, operator.subject(), reason)
            })
            .await?;
        retained::check(prepared.current.client(), facts)?;

        let continuity = continuity.ok_or(RestoreRefusal::ContinuityAbsent)?;
        let reservation = continuity
            .reserve(&Restoring {
                client: id,
                at_revision: expected,
                from: &prepared.from,
                to: &prepared.to,
                operator: operator.subject(),
            })
            .await
            .map_err(|refusal| {
                logging::restore_reservation_refused(id, &refusal);
                refused(&refusal)
            })?;

        let reversed = prepared
            .to
            .restorations
            .last()
            .map(|restoration| restoration.tombstone.clone());
        let stored = match self.write_lifecycle(operator, id, expected, prepared).await {
            Ok(stored) => stored,
            // Only a lost revision proves nothing was written. Any other
            // failure may have landed, so its reservation stays pending for
            // the store to resolve rather than being released over a write.
            Err(ControlPlaneError::RevisionConflict) => {
                continuity.release(&reservation).await;
                return Err(ControlPlaneError::RevisionConflict);
            }
            Err(error) => {
                logging::restore_write_unconfirmed(id, reservation.id());
                return Err(error);
            }
        };

        if let Err(refusal) = continuity.complete(&reservation, &stored.revision).await {
            logging::restore_completion_failed(id, reservation.id(), &refusal);
        }
        if let Some(reversed) = reversed {
            self.audit
                .record(audit::client_restored(operator, id, &stored.revision, &reversed))
                .await;
        }

        Ok(stored)
    }
}

const fn refused(refusal: &ContinuityRefusal) -> RestoreRefusal {
    match refusal {
        ContinuityRefusal::Unenrolled => RestoreRefusal::ContinuityUnenrolled,
        ContinuityRefusal::Mismatch => RestoreRefusal::ContinuityMismatch,
        ContinuityRefusal::Unavailable { .. } => RestoreRefusal::ContinuityUnavailable,
    }
}
