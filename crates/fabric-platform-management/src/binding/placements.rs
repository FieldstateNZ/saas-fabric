//! Delegating placement reads and writes to whatever is bound.
//!
//! The same shape as `binding/data_sources.rs`, which is the same shape as
//! `binding/delegate.rs`: every operation runs in a task that owns the read
//! guard until the delegated call has an outcome, and a revision that
//! leaves here is tagged with the generation it was read at -- including a
//! create's tagged absence, so a rebind between the read and the write is
//! caught the same way for a placements create as for a data-sources one.
//! See `binding/delegate.rs` and `binding/generation.rs` for the reasoning;
//! nothing here repeats it, only its shape.

use crate::binding::holding::outliving;
use crate::binding::{generation, PlatformDesiredState};
use crate::{DesiredRevision, DesiredStateError, PlacementRecord, PlacementState, PlacementsRead};

#[async_trait::async_trait]
impl PlacementState for PlatformDesiredState {
    async fn read_placements(&self, environment: &str) -> Result<PlacementsRead, DesiredStateError> {
        let live = self.held().await;
        let repository = live.placement_repository()?;
        let read_at = live.generation();
        let environment = environment.to_owned();

        let mut read = outliving(
            live,
            async move { repository.read_placements(&environment).await },
        )
        .await?;

        read.revision = Some(generation::tag_presence(read_at, read.revision.as_ref()));

        Ok(read)
    }

    async fn write_placements(
        &self,
        environment: &str,
        placements: &[PlacementRecord],
        at: Option<&DesiredRevision>,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        let live = self.held().await;
        let repository = live.placement_repository()?;

        let at = generation::untag_presence(live.generation(), at.ok_or(DesiredStateError::Conflict)?)?;

        let (environment, placements, message) =
            (environment.to_owned(), placements.to_vec(), message.to_owned());

        outliving(live, async move {
            repository
                .write_placements(&environment, &placements, at.as_ref(), &message)
                .await
        })
        .await
    }
}
