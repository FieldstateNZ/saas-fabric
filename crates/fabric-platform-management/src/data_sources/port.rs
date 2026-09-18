//! Where an environment's declared data sources are kept.

use crate::data_sources::declaration::DataSourceDeclaration;
use crate::data_sources::read::DataSourcesRead;
use crate::{DesiredRevision, DesiredStateError};

/// Where an environment's declared data sources are read and written.
///
/// Implemented by an adapter that knows how the platform repository lays
/// out environments/ENV/data-sources.yaml. Nothing here knows the file
/// exists -- the same separation `DesiredState` keeps between the rules and
/// the transport.
#[async_trait::async_trait]
pub trait DataSourceState: Send + Sync {
    /// Every data source an environment declares, and the revision it was
    /// read at.
    ///
    /// # Errors
    ///
    /// `DesiredStateError` if the environment cannot be read.
    async fn read_data_sources(&self, environment: &str) -> Result<DataSourcesRead, DesiredStateError>;

    /// Replaces an environment's declared data sources with exactly this
    /// list.
    ///
    /// at is the revision this write was decided against: None means
    /// create -- refuse if the file exists, and Some(revision) means
    /// replace -- refuse unless the file is still at that revision.
    ///
    /// # Why a whole-document replace rather than per-entry mutation
    ///
    /// `DataSources::declare` decides the complete list an environment
    /// should hold, and this hands the whole of it back in one call. That
    /// mirrors the file itself, which is rewritten whole on every change
    /// so a hand edit survives as values and not as formatting.
    ///
    /// # Errors
    ///
    /// Conflict if the state moved since at was read, and the other
    /// `DesiredStateError` variants for what they name.
    async fn write_data_sources(
        &self,
        environment: &str,
        declarations: &[DataSourceDeclaration],
        at: Option<&DesiredRevision>,
        message: &str,
    ) -> Result<(), DesiredStateError>;
}
