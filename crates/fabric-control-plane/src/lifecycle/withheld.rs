//! How the control plane answers [`TenantLifecycleSource`]: every client
//! whose recorded lifecycle refuses publication.

use std::collections::BTreeSet;
use std::sync::Arc;

use fabric_client_model::LifecycleUse;
use fabric_core::TenantId;
use fabric_platform_management::{LifecycleSourceError, TenantLifecycleSource};

use crate::repository::{DesiredStateBinding, RepositoryError};
use crate::ControlPlaneError;

/// Reads client desired state through the same binding
/// [`crate::ClientService`] reads, for the reason
/// `DesiredStateCatalogueSource` does: this crate is where the client
/// documents and the platform publisher meet.
pub(crate) struct DesiredStateLifecycleSource {
    repository: Arc<DesiredStateBinding>,
}

impl DesiredStateLifecycleSource {
    /// Reads through `repository`, late-bound.
    pub(crate) fn new(repository: Arc<DesiredStateBinding>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl TenantLifecycleSource for DesiredStateLifecycleSource {
    async fn withheld_tenants(&self) -> Result<BTreeSet<TenantId>, LifecycleSourceError> {
        let clients = self.repository.current().list().await.map_err(|error| {
            let invalid = matches!(error, RepositoryError::Invalid { .. });
            // The same translation every other repository failure in this
            // crate goes through, so no path, branch or upstream body
            // reaches the publication row.
            let message = ControlPlaneError::from_repository(error).public_message();
            if invalid {
                LifecycleSourceError::Invalid(message)
            } else {
                LifecycleSourceError::Unavailable(message)
            }
        })?;

        let mut withheld = BTreeSet::new();
        for stored in clients {
            if super::require(&stored.document, LifecycleUse::Publish).is_err() {
                let id = &stored.document.client().id;
                let tenant = TenantId::try_new(id.as_str()).map_err(|_| {
                    LifecycleSourceError::Invalid(format!("client {id} is not a valid tenant id"))
                })?;
                withheld.insert(tenant);
            }
        }

        Ok(withheld)
    }
}
