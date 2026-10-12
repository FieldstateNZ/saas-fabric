//! The two halves of every lifecycle write: preparing the transition at the
//! revision the operator read, then writing it. Split so a restore can check
//! retained placements and reserve the transition in the continuity store
//! between them, before anything is written.

use fabric_client_model::catalogue::ProductActivity;
use fabric_client_model::{ClientDocument, ClientId, ClientRevision, LifecycleRecord, LifecycleRefusal};

use crate::{document_size, ChangeContext, ClientService, ControlPlaneError, Operator, StoredClient};

/// A lifecycle transition decided and rendered, not yet written.
pub(super) struct Prepared {
    /// The lifecycle the document records now.
    pub(super) from: LifecycleRecord,
    /// The lifecycle the write will record.
    pub(super) to: LifecycleRecord,
    /// The client as stored now.
    pub(super) current: ClientDocument,
    /// The document the write will store.
    document: ClientDocument,
    /// The activity entry's action, in an operator's words.
    action: &'static str,
}

impl ClientService {
    /// Reads the client at `expected` and applies one lifecycle transition,
    /// with an activity entry, writing nothing.
    pub(super) async fn prepare_lifecycle(
        &self,
        operator: &Operator,
        id: &ClientId,
        expected: &ClientRevision,
        action: &'static str,
        transition: impl FnOnce(&LifecycleRecord) -> Result<LifecycleRecord, LifecycleRefusal>,
    ) -> Result<Prepared, ControlPlaneError> {
        let current = self
            .repository
            .current()
            .get(id)
            .await
            .map_err(ControlPlaneError::from_repository)?;
        if &current.revision != expected {
            return Err(ControlPlaneError::RevisionConflict);
        }

        let from = current.document.lifecycle().clone();
        let to = transition(&from)?;
        let document = current
            .document
            .with_lifecycle(&to)
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
        // near the ordinary limit must still be retirable and restorable.
        document_size::check_remediation(&document.render().map_err(ControlPlaneError::InvalidRequest)?)?;

        Ok(Prepared {
            from,
            to,
            current: current.document,
            document,
            action,
        })
    }

    /// Writes `prepared` if the client is still at `expected`, and marks
    /// reconciliation pending at the revision it produced.
    pub(super) async fn write_lifecycle(
        &self,
        operator: &Operator,
        id: &ClientId,
        expected: &ClientRevision,
        prepared: Prepared,
    ) -> Result<StoredClient, ControlPlaneError> {
        let change = ChangeContext {
            requested_by: operator.subject().into(),
            summary: format!("{} {id}", prepared.action.to_lowercase()),
        };
        let revision = self
            .repository
            .current()
            .update(id, &prepared.document, expected, &change)
            .await
            .map_err(ControlPlaneError::from_repository)?;
        self.reconciliation
            .mark_pending(id, revision.clone(), self.clock.now_unix_seconds());

        Ok(StoredClient {
            document: prepared.document,
            revision,
        })
    }
}
