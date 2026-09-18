//! Where an environment's recorded placements are kept.

use crate::placements::read::PlacementsRead;
use crate::placements::record::PlacementRecord;
use crate::{DesiredRevision, DesiredStateError};

/// Where an environment's recorded placements are read and written.
///
/// [`DataSourceState`](crate::DataSourceState)'s sibling: implemented by an
/// adapter that knows how the platform repository lays out
/// `environments/ENV/placements.yaml`. Nothing here knows the file exists.
#[async_trait::async_trait]
pub trait PlacementState: Send + Sync {
    /// Every placement an environment has recorded, and the revision it
    /// was read at.
    ///
    /// # Errors
    ///
    /// `DesiredStateError` if the environment cannot be read.
    async fn read_placements(&self, environment: &str) -> Result<PlacementsRead, DesiredStateError>;

    /// Replaces an environment's recorded placements with exactly this
    /// list.
    ///
    /// `at` is the revision this write was decided against: `None` means
    /// create -- refuse if the file exists -- and `Some(revision)` means
    /// replace -- refuse unless the file is still at that revision. A
    /// whole-document replace for the same reason
    /// [`DataSourceState::write_data_sources`](crate::DataSourceState::write_data_sources)
    /// is: the file is rewritten whole on every change, so a hand edit
    /// under break-glass survives as values and not as formatting.
    ///
    /// # Errors
    ///
    /// Conflict if the state moved since `at` was read, and the other
    /// `DesiredStateError` variants for what they name.
    async fn write_placements(
        &self,
        environment: &str,
        placements: &[PlacementRecord],
        at: Option<&DesiredRevision>,
        message: &str,
    ) -> Result<(), DesiredStateError>;
}
