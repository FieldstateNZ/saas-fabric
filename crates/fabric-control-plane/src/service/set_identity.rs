//! Changing a client's identity, which means writing a document to Git.

use fabric_client_model::{ClientId, ClientRevision, IdentityConfiguration};

use crate::repository::{ChangeContext, StoredClient};
use crate::{audit, ClientService, ControlPlaneError, Operator};

impl ClientService {
    /// Replaces a client's identity configuration.
    ///
    /// # What this does, in order, and why the order is the design
    ///
    /// 1. Read the client at its current revision.
    /// 2. Refuse the request if that is not the revision the operator was
    ///    editing.
    /// 3. Refuse a realm change — reconciliation cannot express one safely.
    /// 4. Merge the new identity into the *stored document*, so nothing this
    ///    model does not understand is disturbed.
    /// 5. Return early if nothing actually changed, rather than writing a
    ///    commit that says nothing and resetting the client to `pending`.
    /// 6. Write, which refuses again if the revision moved in between.
    /// 7. Mark reconciliation pending, then ask for a pass.
    /// 8. Record who did what.
    ///
    /// Step 2 comes **before** step 5, and the order is load-bearing. A stale
    /// `If-Match` is refused even when the change would have been a no-op:
    /// letting an identical body through would make the precondition mean
    /// "unless it does not matter", which is not something a caller could
    /// reason about. It also does not make the repository's own check
    /// redundant — that one is atomic and closes the window between this read
    /// and the write below.
    ///
    /// Step 7 is the one worth defending. Marking pending happens **before**
    /// the reconciliation loop is nudged and regardless of whether it ever
    /// runs, so the status is honest from the instant the write lands. If it
    /// were set by the loop instead, a client whose reconciliation was broken
    /// would go on displaying the previous revision's `applied` — the exact
    /// false reassurance ADR 0008 exists to prevent.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError`] if the client does not exist, the request
    /// would move the realm, the identity breaks a validation rule, the
    /// revision has moved on — [`ControlPlaneError::RevisionConflict`] — or
    /// the repository could not be written.
    pub async fn set_identity(
        &self,
        operator: &Operator,
        client: &ClientId,
        identity: IdentityConfiguration,
        expected: &ClientRevision,
    ) -> Result<StoredClient, ControlPlaneError> {
        let current = self.get(client).await?;

        if current.revision != *expected {
            return Err(ControlPlaneError::RevisionConflict);
        }

        let existing = current.document.client();

        if existing.identity.realm != identity.realm {
            return Err(ControlPlaneError::RealmImmutable {
                current: existing.identity.realm.clone(),
            });
        }

        if existing.identity == identity {
            return Ok(current);
        }

        let updated = current
            .document
            .with_identity(identity)
            .map_err(ControlPlaneError::InvalidRequest)?;

        let change = ChangeContext {
            requested_by: operator.subject().to_owned(),
            summary: format!("update identity for {client}"),
        };

        let revision = self
            .repository
            .current()
            .update(client, &updated, expected, &change)
            .await
            .map_err(ControlPlaneError::from_repository)?;

        self.reconciliation
            .mark_pending(client, revision.clone(), self.clock.now_unix_seconds());

        audit::identity_updated(operator, client, &revision);

        Ok(StoredClient {
            document: updated,
            revision,
        })
    }
}
