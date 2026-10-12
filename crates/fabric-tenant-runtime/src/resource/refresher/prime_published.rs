//! Priming the published runtime: the first view installed, whole.

use super::install::{install, Installed, Registries};
use super::ResourceRefresher;
use crate::publication::{PublishedStateSource, ReaderStatus};
use crate::{logging, DataSource, SourceError, TenantRuntimeBinding};

impl ResourceRefresher {
    /// Loads the published documents once and installs them as the first
    /// view: a complete generation whole, catalogue included, or documents
    /// with no record as one view under no generation.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if nothing could be installed. No view is installed
    /// then, so requests answer that the runtime is unavailable, and the
    /// readiness detail reports the hold.
    pub(crate) async fn prime_published(
        registries: Registries<'_>,
        source: &dyn PublishedStateSource,
        status: &ReaderStatus,
    ) -> Result<(), SourceError> {
        let origin = source.describe();
        let installed = source
            .load_at_start()
            .await
            .and_then(|state| install(state, registries, &origin));
        match installed {
            Ok(installed) => {
                let (_, data_sources, tenants) = registries;
                logging::primed::<DataSource>(&origin, data_sources.len());
                logging::primed::<TenantRuntimeBinding>(&origin, tenants.len());
                settle(installed, source, status).await;
                Ok(())
            }
            Err(error) => {
                status.held(&error);
                Err(error)
            }
        }
    }
}

/// Tells the source and the status what was installed, after it was.
pub(super) async fn settle(installed: Installed, source: &dyn PublishedStateSource, status: &ReaderStatus) {
    if let Some((generation, record_sha256)) = installed.recorded {
        logging::generation_applied(&source.describe(), generation);
        source.installed(generation, &record_sha256).await;
    }
    status.applied(installed.serving);
}
