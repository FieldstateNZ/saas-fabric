//! The revision a data source was last published at, for a declaration to
//! be placed above.

use std::sync::Arc;

use fabric_core::DataSourceId;
use fabric_runtime_publication::RuntimePublication;

use crate::data_sources::plan::Published;
use crate::data_sources::service::DataSources;
use crate::PlatformError;

impl DataSources {
    /// Places every declaration's revision above the highest revision the
    /// same id has been published at through `target`.
    #[must_use]
    pub fn with_publication(self, target: Arc<dyn RuntimePublication>) -> Self {
        Self {
            published: Some(target),
            ..self
        }
    }

    /// The highest revision `id` has been published at and whether it is
    /// still published, or `None` with no publication configured or where
    /// it was never published.
    pub(super) async fn published_revision(
        &self,
        id: &DataSourceId,
    ) -> Result<Option<Published>, PlatformError> {
        let Some(target) = &self.published else {
            return Ok(None);
        };
        let resources = target.published_resources().await.map_err(|error| {
            tracing::warn!(%error, "could not read the published runtime state for a declaration");
            PlatformError::PublishedStateUnreadable
        })?;
        Ok(resources.data_source_revision(id).map(|revision| Published {
            revision,
            current: resources.data_sources.iter().any(|published| published.id == *id),
        }))
    }
}
