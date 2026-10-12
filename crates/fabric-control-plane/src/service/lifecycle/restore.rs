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
//! 4. with no continuity store the restore is held; otherwise the store
//!    reserves the transition, bound to the identity instance, sequence,
//!    reviewed revision, starting lifecycle and the tombstone reversed;
//! 5. the document is written at that revision, and reconciliation is marked
//!    pending at the new one;
//! 6. the reservation is completed and, once it is, the durable trail
//!    records the restore. Until it is, the client stays held, and the same
//!    request sent again completes it (`record`).
//!
//! The restore touches nothing else: no placement, tenant binding, resource
//! history, secret or identity provider. Identity comes back only when a
//! pass applies the new revision with a consistent projection.

use fabric_client_model::{ClientId, ClientRevision};

use super::record::Confirmed;
use crate::lifecycle::retained::{self, RetainedEvidence};
use crate::{
    audit, ClientService, ContinuityHold, ControlPlaneError, LifecycleContinuity, Operator, StoredClient,
    TransitionKind,
};

impl ClientService {
    /// Restores a tombstoned client, retaining its tombstone in history.
    ///
    /// # Errors
    ///
    /// [`ControlPlaneError::RevisionConflict`] if `expected` is stale;
    /// [`ControlPlaneError::Lifecycle`] if the client is not tombstoned, its
    /// deletion is confirmed, or the reason is not acceptable;
    /// [`ControlPlaneError::Restore`] if its placements were not retained;
    /// and [`ControlPlaneError::Continuity`] if there is no continuity store
    /// or it refuses. Nothing is written after any of these.
    pub(crate) async fn restore(
        &self,
        operator: &Operator,
        id: &ClientId,
        reason: &str,
        expected: &ClientRevision,
        evidence: &RetainedEvidence,
        continuity: Option<&dyn LifecycleContinuity>,
    ) -> Result<StoredClient, ControlPlaneError> {
        let kind = TransitionKind::Restore;
        if let Some(recovered) = self.recover_transition(id, expected, continuity, kind).await? {
            self.audit_restore(operator, id, &recovered).await;
            return Ok(recovered);
        }
        let at = self.clock.now_unix_seconds();
        let prepared = self
            .prepare_lifecycle(operator, id, expected, "Client restored", |record| {
                record.restore(at, operator.subject(), reason)
            })
            .await?;
        retained::check(prepared.current.client(), evidence)?;
        if continuity.is_none() {
            return Err(ContinuityHold::NoStore.into());
        }

        let (stored, confirmed) = self
            .record_transition(operator, id, expected, prepared, continuity, kind)
            .await?;
        if confirmed == Confirmed::Yes {
            self.audit_restore(operator, id, &stored).await;
        }

        Ok(stored)
    }

    /// Audits a confirmed restore, naming the tombstone it reversed.
    async fn audit_restore(&self, operator: &Operator, id: &ClientId, stored: &StoredClient) {
        if let Some(restoration) = stored.document.lifecycle().restorations.last() {
            self.audit
                .record(audit::client_restored(
                    operator,
                    id,
                    &stored.revision,
                    &restoration.tombstone,
                ))
                .await;
        }
    }
}
