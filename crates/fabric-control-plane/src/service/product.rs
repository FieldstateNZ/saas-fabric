//! Client creation and configuration are single-document desired-state writes.
use crate::{ChangeContext, ClientService, ControlPlaneError, Operator, StoredClient};
use fabric_client_model::{
    catalogue::{ClientProductRequest, CreateClientRequest, ProductActivity},
    ClientDocument, ClientId, ClientRevision, DesiredStateError,
};
impl ClientService {
    /// Creates a client, checking duplicates atomically in the repository.
    /// # Errors
    /// Refuses invalid assignments, occupied identifiers and repository failures.
    pub async fn create_client(
        &self,
        operator: &Operator,
        request: CreateClientRequest,
    ) -> Result<StoredClient, ControlPlaneError> {
        let repository = self.repository.current();
        let catalogue = repository
            .catalogue()
            .await
            .map_err(ControlPlaneError::from_repository)?;
        let mut product = catalogue
            .catalogue
            .resolve(&request.configuration)
            .map_err(ControlPlaneError::InvalidRequest)?;
        product
            .activity
            .push(self.product_event(operator, &request.id, "Client created"));
        let document = ClientDocument::create(&request.id, &request.configuration, &product)
            .map_err(ControlPlaneError::InvalidRequest)?;
        let change = ChangeContext {
            requested_by: operator.subject().into(),
            summary: format!("create client {}", request.id),
        };
        let revision = repository
            .create(&document, &change)
            .await
            .map_err(ControlPlaneError::from_repository)?;
        self.reconciliation
            .mark_pending(&request.id, revision.clone(), self.clock.now_unix_seconds());
        Ok(StoredClient { document, revision })
    }
    /// Replaces editable client fields and pins requested application releases.
    /// # Errors
    /// Requires the current revision; refuses destructive removal until deprovisioning exists.
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
        let previous = current
            .document
            .product()
            .map_err(ControlPlaneError::InvalidRequest)?;
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
        let mut product = catalogue
            .catalogue
            .resolve(&request)
            .map_err(ControlPlaneError::InvalidRequest)?;
        product.activity = previous.activity;
        product
            .activity
            .push(self.product_event(operator, id, "Client configuration updated"));
        let document = current
            .document
            .with_product(&request, &product)
            .map_err(ControlPlaneError::InvalidRequest)?;
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
