//! Implementing the recorded-placements port over the platform repository.
//!
//! The same budget contract `port.rs` states for `DesiredState` applies here
//! too, for the same reason it applies to `port/data_sources.rs`: the
//! platform binding holds a lock across this call regardless of which port
//! an operation belongs to.

pub(crate) mod document;
mod header;
pub(crate) mod read;
pub(crate) mod write;

use fabric_platform_management::{
    DesiredRevision, DesiredStateError, PlacementRecord, PlacementState, PlacementsRead,
};

use crate::host::PlatformGitRepository;

#[async_trait::async_trait]
impl PlacementState for PlatformGitRepository {
    async fn read_placements(&self, environment: &str) -> Result<PlacementsRead, DesiredStateError> {
        self.within_budget(async {
            let read = self.read_placements_file(environment).await?;

            Ok(match read.file {
                None => PlacementsRead {
                    revision: None,
                    placements: Vec::new(),
                },
                Some((stored, document)) => PlacementsRead {
                    revision: Some(DesiredRevision::new(stored.revision.as_str())),
                    placements: document.into_placements(),
                },
            })
        })
        .await
    }

    async fn write_placements(
        &self,
        environment: &str,
        placements: &[PlacementRecord],
        at: Option<&DesiredRevision>,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.within_budget(async {
            self.write_placements_file(environment, placements, at, message)
                .await?;

            Ok(())
        })
        .await
    }
}
