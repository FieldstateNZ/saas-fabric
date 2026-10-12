//! The two lifecycle writes: tombstoning a client, then, separately,
//! confirming its deletion (D01-14).
//!
//! Neither removes anything. A tombstone retains every section of the
//! document, every placement record, the realm and the secrets; a confirmed
//! deletion records that an operator confirmed it. What deletion would
//! remove, and after what retention, is not decided.

use fabric_client_model::catalogue::ProductActivity;
use fabric_client_model::{ClientId, ClientRevision, LifecycleRecord, LifecycleRefusal};

use crate::{audit, document_size, ChangeContext, ClientService, ControlPlaneError, Operator, StoredClient};

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
        let stored = self
            .record_lifecycle(operator, id, expected, "Client tombstoned", |record| {
                record.tombstone(at, operator.subject(), reason)
            })
            .await?;
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
        let stored = self
            .record_lifecycle(operator, id, expected, "Client deletion confirmed", |record| {
                record.confirm_deletion(id, confirming, at, operator.subject())
            })
            .await?;
        self.audit
            .record(audit::client_deletion_confirmed(operator, id, &stored.revision))
            .await;

        Ok(stored)
    }

    /// Reads the client at `expected`, applies one lifecycle transition, and
    /// writes the result with an activity entry.
    async fn record_lifecycle(
        &self,
        operator: &Operator,
        id: &ClientId,
        expected: &ClientRevision,
        action: &str,
        transition: impl FnOnce(&LifecycleRecord) -> Result<LifecycleRecord, LifecycleRefusal>,
    ) -> Result<StoredClient, ControlPlaneError> {
        let repository = self.repository.current();
        let current = repository
            .get(id)
            .await
            .map_err(ControlPlaneError::from_repository)?;
        if &current.revision != expected {
            return Err(ControlPlaneError::RevisionConflict);
        }

        let record = transition(current.document.lifecycle())?;
        let document = current
            .document
            .with_lifecycle(&record)
            .map_err(ControlPlaneError::InvalidRequest)?
            .with_activity(ProductActivity {
                at: self.clock.now_unix_seconds(),
                operator: operator.subject().into(),
                action: action.into(),
                resource: id.to_string(),
            })
            .map_err(|source| ControlPlaneError::InvalidDesiredState {
                client: id.clone(),
                source,
            })?;

        // The remediation limit, as an identity edit uses: a document already
        // near the ordinary limit must still be retirable.
        document_size::check_remediation(&document.render().map_err(ControlPlaneError::InvalidRequest)?)?;

        let change = ChangeContext {
            requested_by: operator.subject().into(),
            summary: format!("{} {id}", action.to_lowercase()),
        };
        let revision = repository
            .update(id, &document, expected, &change)
            .await
            .map_err(ControlPlaneError::from_repository)?;
        self.reconciliation
            .mark_pending(id, revision.clone(), self.clock.now_unix_seconds());

        Ok(StoredClient { document, revision })
    }
}
