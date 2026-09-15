//! Replacing a client's product configuration is a single desired-state
//! write, over the client's own existing copy of every release it keeps.

use fabric_client_model::{
    catalogue::{ClientProductRequest, ProductActivity},
    ClientId, ClientRevision, DesiredStateError,
};

use crate::{audit, document_size, ChangeContext, ClientService, ControlPlaneError, Operator, StoredClient};

impl ClientService {
    /// Replaces editable client fields and pins requested application releases.
    ///
    /// # Errors
    ///
    /// Requires the current revision; refuses destructive removal until
    /// deprovisioning exists, and a document too large to store.
    pub async fn set_product(
        &self,
        operator: &Operator,
        id: &ClientId,
        request: ClientProductRequest,
        expected: &ClientRevision,
    ) -> Result<StoredClient, ControlPlaneError> {
        let repository = self.repository.current();
        let current = repository
            .get(id)
            .await
            .map_err(ControlPlaneError::from_repository)?;
        if &current.revision != expected {
            return Err(ControlPlaneError::RevisionConflict);
        }
        // `current` came from the repository, not from this request, so a
        // product section that will not parse is a stored document the
        // platform cannot read — the same failure `GET /api/clients` reports
        // for a client document that will not parse at all — not something
        // this write asked for.
        let previous =
            current
                .document
                .product()
                .map_err(|source| ControlPlaneError::InvalidDesiredState {
                    client: id.clone(),
                    source,
                })?;
        if previous.applications.iter().any(|a| {
            !request
                .applications
                .iter()
                .any(|r| r.application_id == a.application_id)
        }) {
            return Err(ControlPlaneError::InvalidRequest(
                DesiredStateError::InvalidField {
                    field: "applications",
                    detail: "Application removal requires deprovisioning and is not supported".into(),
                },
            ));
        }
        let catalogue = repository
            .catalogue()
            .await
            .map_err(ControlPlaneError::from_repository)?;
        // The client's own previous assignments, not the catalogue, decide
        // which release an unchanged assignment keeps — see `Catalogue::resolve`.
        let mut product = catalogue
            .catalogue
            .resolve(&request, &previous.applications)
            .map_err(ControlPlaneError::InvalidRequest)?;
        product.activity = previous.activity;
        product
            .activity
            .push(self.product_event(operator, id, "Client configuration updated"));
        let document = current
            .document
            .with_product(&request, &product)
            .map_err(ControlPlaneError::InvalidRequest)?;

        // Measured before the write, not left to the repository to
        // discover: GitHub's contents API cannot read a file this size
        // back, so a document that grows past it must be refused here
        // rather than committed and then unreadable.
        document_size::check(&document.render().map_err(ControlPlaneError::InvalidRequest)?)?;

        let change = ChangeContext {
            requested_by: operator.subject().into(),
            summary: format!("update client {id} configuration"),
        };
        let revision = repository
            .update(id, &document, expected, &change)
            .await
            .map_err(ControlPlaneError::from_repository)?;
        self.reconciliation
            .mark_pending(id, revision.clone(), self.clock.now_unix_seconds());

        audit::product_updated(operator, id, &revision);

        Ok(StoredClient { document, revision })
    }

    pub(super) fn product_event(&self, operator: &Operator, id: &ClientId, action: &str) -> ProductActivity {
        ProductActivity {
            at: self.clock.now_unix_seconds(),
            operator: operator.subject().into(),
            action: action.into(),
            resource: id.to_string(),
        }
    }
}
