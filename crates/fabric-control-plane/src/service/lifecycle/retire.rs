//! The two retiring lifecycle writes: tombstoning a client, then,
//! separately, confirming its deletion (D01-14). Restoring is in `restore`.
//!
//! Neither removes anything. A tombstone retains every section of the
//! document, every placement record, the realm and the secrets; a confirmed
//! deletion records that an operator confirmed it. What deletion would
//! remove, and after what retention, is not decided.

use fabric_client_model::{ClientId, ClientRevision};

use crate::{audit, ClientService, ControlPlaneError, Operator, StoredClient};

impl ClientService {
    /// Tombstones a client, retaining its state.
    ///
    /// # Errors
    ///
    /// [`ControlPlaneError::RevisionConflict`] if `expected` is stale, and
    /// [`ControlPlaneError::Lifecycle`] if the client is already retired or
    /// the reason is not acceptable.
    pub async fn tombstone(
        &self,
        operator: &Operator,
        id: &ClientId,
        reason: &str,
        expected: &ClientRevision,
    ) -> Result<StoredClient, ControlPlaneError> {
        let at = self.clock.now_unix_seconds();
        let prepared = self
            .prepare_lifecycle(operator, id, expected, "Client tombstoned", |record| {
                record.tombstone(at, operator.subject(), reason)
            })
            .await?;
        let stored = self.write_lifecycle(operator, id, expected, prepared).await?;
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
    /// the client was never tombstoned, or deletion is already confirmed.
    pub async fn confirm_deletion(
        &self,
        operator: &Operator,
        id: &ClientId,
        confirming: &ClientId,
        expected: &ClientRevision,
    ) -> Result<StoredClient, ControlPlaneError> {
        let at = self.clock.now_unix_seconds();
        let prepared = self
            .prepare_lifecycle(operator, id, expected, "Client deletion confirmed", |record| {
                record.confirm_deletion(id, confirming, at, operator.subject())
            })
            .await?;
        let stored = self.write_lifecycle(operator, id, expected, prepared).await?;
        self.audit
            .record(audit::client_deletion_confirmed(operator, id, &stored.revision))
            .await;

        Ok(stored)
    }
}
