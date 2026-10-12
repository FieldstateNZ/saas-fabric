//! The two retiring lifecycle writes: tombstoning a client, then,
//! separately, confirming its deletion (D01-14). Restoring is in `restore`.
//!
//! Neither removes anything. A tombstone retains every section of the
//! document, every placement record, the realm and the secrets; a confirmed
//! deletion records that an operator confirmed it. What deletion would
//! remove, and after what retention, is not decided.
//!
//! With a continuity store each is reserved and completed like any other
//! transition, so a later Git revert of it reads as a regression; a refused
//! reservation refuses the write. With none, each writes desired state only:
//! it restricts, and no client is admitted without a store anyway.

use fabric_client_model::{ClientId, ClientRevision};

use super::record::{Asked, Ran};
use crate::{
    audit, ClientService, ControlPlaneError, LifecycleContinuity, Operator, StoredClient, TransitionKind,
};

impl ClientService {
    /// Tombstones a client, retaining its state.
    ///
    /// # Errors
    ///
    /// [`ControlPlaneError::RevisionConflict`] if `expected` is stale, and
    /// [`ControlPlaneError::Lifecycle`] if the client is already retired or
    /// the reason is not acceptable, and [`ControlPlaneError::Continuity`]
    /// if a continuity store refuses the reservation.
    pub async fn tombstone(
        &self,
        operator: &Operator,
        id: &ClientId,
        reason: &str,
        expected: &ClientRevision,
        continuity: Option<&dyn LifecycleContinuity>,
    ) -> Result<StoredClient, ControlPlaneError> {
        let at = self.clock.now_unix_seconds();
        let asked = Asked {
            operator,
            id,
            expected,
            kind: TransitionKind::Tombstone,
            action: "Client tombstoned",
            reason: Some(reason),
            continuity,
        };
        let stored = match self
            .run_transition(
                &asked,
                |record| record.tombstone(at, operator.subject(), reason),
                |_| Ok(()),
            )
            .await?
        {
            Ran::Recovered(stored) => return Ok(stored),
            Ran::Written(stored, _) => stored,
        };
        self.audit
            .record(audit::client_tombstoned(operator, id, &stored.revision))
            .await;

        Ok(stored)
    }

    /// Confirms deletion of a tombstoned client. Records intent only.
    ///
    /// # Errors
    ///
    /// [`ControlPlaneError::RevisionConflict`] if `expected` is stale, and
    /// [`ControlPlaneError::Lifecycle`] if `confirming` is not this client,
    /// the client was never tombstoned, or deletion is already confirmed, and
    /// [`ControlPlaneError::Continuity`] if a continuity store refuses.
    pub async fn confirm_deletion(
        &self,
        operator: &Operator,
        id: &ClientId,
        confirming: &ClientId,
        expected: &ClientRevision,
        continuity: Option<&dyn LifecycleContinuity>,
    ) -> Result<StoredClient, ControlPlaneError> {
        let at = self.clock.now_unix_seconds();
        let asked = Asked {
            operator,
            id,
            expected,
            kind: TransitionKind::ConfirmDeletion,
            action: "Client deletion confirmed",
            reason: None,
            continuity,
        };
        let confirm = |record: &fabric_client_model::LifecycleRecord| {
            record.confirm_deletion(id, confirming, at, operator.subject())
        };
        let stored = match self.run_transition(&asked, confirm, |_| Ok(())).await? {
            Ran::Recovered(stored) => return Ok(stored),
            Ran::Written(stored, _) => stored,
        };
        self.audit
            .record(audit::client_deletion_confirmed(operator, id, &stored.revision))
            .await;

        Ok(stored)
    }
}
