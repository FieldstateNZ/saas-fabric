//! How the control plane answers [`RuntimeCatalogueSource`] (ADR 0023 part
//! 4): the derived runtime catalogue, read through the same binding every
//! other client-facing read already uses.

use std::sync::Arc;

use fabric_client_model::catalogue::CatalogueConflict;
use fabric_platform_management::{CatalogueSourceError, RuntimeCatalogueSource};
use fabric_runtime_publication::CatalogDocument;

use crate::repository::DesiredStateBinding;

/// Reads the environment's derived runtime catalogue through the *client*
/// desired-state binding -- the same one [`crate::ClientService`] reads, not
/// the platform repository [`crate::PlatformBinding`] holds.
///
/// # Why this lives here, and not on `ClientService`
///
/// [`RuntimeCatalogueSource`] exists so `fabric-platform-management`'s
/// publisher never has to depend on `fabric-client-model` -- see that
/// trait's own rustdoc. This crate is the one place both the client
/// desired-state binding and the platform publisher meet, so it is where
/// the seam is closed: a small, standalone adapter, because nothing about
/// hanging this off the client domain service would make it easier to
/// build, test, or find.
pub(crate) struct DesiredStateCatalogueSource {
    repository: Arc<DesiredStateBinding>,
}

impl DesiredStateCatalogueSource {
    /// Reads through `repository`, late-bound the same way every other read
    /// in this crate is -- an operator connecting or disconnecting desired
    /// state while a publication pass is in flight is answered honestly
    /// rather than against a repository captured at construction.
    pub(crate) fn new(repository: Arc<DesiredStateBinding>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl RuntimeCatalogueSource for DesiredStateCatalogueSource {
    async fn runtime_catalogue(&self) -> Result<CatalogDocument, CatalogueSourceError> {
        let stored = self
            .repository
            .current()
            .catalogue()
            .await
            .map_err(|error| CatalogueSourceError::Unavailable(error.to_string()))?;

        let derived = stored
            .catalogue
            .runtime_catalogue()
            .map_err(|conflict: CatalogueConflict| CatalogueSourceError::Conflict {
                resource: conflict.resource.to_string(),
                applications: (
                    conflict.applications.0.to_string(),
                    conflict.applications.1.to_string(),
                ),
            })?;

        Ok(derived.into_document())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::InMemoryClientRepository;

    #[tokio::test]
    async fn an_unbound_repository_answers_unavailable_naming_no_path() {
        let source = DesiredStateCatalogueSource::new(DesiredStateBinding::unconfigured());

        let error = source
            .runtime_catalogue()
            .await
            .expect_err("nothing is bound yet");

        assert!(matches!(error, CatalogueSourceError::Unavailable(_)));
        let CatalogueSourceError::Unavailable(detail) = error else {
            unreachable!("matched above");
        };
        assert_eq!(detail, "no desired-state repository is configured");
    }

    #[tokio::test]
    async fn a_repository_with_nothing_published_answers_an_empty_catalogue() {
        let repository = Arc::new(InMemoryClientRepository::new());
        let source = DesiredStateCatalogueSource::new(DesiredStateBinding::to(repository));

        let document = source
            .runtime_catalogue()
            .await
            .expect("an empty catalogue is not a conflict");

        assert!(document.is_empty());
    }
}
