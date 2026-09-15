//! Durable reconciliation history uses the same conditional repository writes.
use crate::{ChangeContext, ClientService, ControlPlaneError, Operator, RepositoryError};
use fabric_client_model::catalogue::ProductActivity;
impl ClientService {
    pub(crate) async fn record_convergence(
        &self,
        operator: &Operator,
        clients: usize,
    ) -> Result<(), ControlPlaneError> {
        let repository = self.repository.current();
        for _ in 0..3 {
            let mut current = repository
                .catalogue()
                .await
                .map_err(ControlPlaneError::from_repository)?;
            current.catalogue.activity.push(ProductActivity {
                at: self.clock.now_unix_seconds(),
                operator: operator.subject().into(),
                action: format!(
                    "Identity reconciliation pass completed for {clients} clients; inspect client outcomes"
                ),
                resource: "reconciliation".into(),
            });
            let change = ChangeContext {
                requested_by: operator.subject().into(),
                summary: "record identity reconciliation pass".into(),
            };
            match repository
                .save_catalogue(&current.catalogue, current.revision.as_ref(), &change)
                .await
            {
                Ok(_) => return Ok(()),
                Err(RepositoryError::Conflict) => {}
                Err(error) => return Err(ControlPlaneError::from_repository(error)),
            }
        }
        Err(ControlPlaneError::RevisionConflict)
    }
}
