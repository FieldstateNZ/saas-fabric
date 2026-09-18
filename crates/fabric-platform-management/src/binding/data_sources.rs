//! Delegating declared-data-source reads and writes to whatever is bound.
//!
//! The same shape as `binding/delegate.rs`: every operation runs in a task
//! that owns the read guard until the delegated call has an outcome. A
//! revision that leaves here is tagged with the generation it was read at,
//! and one arriving on a write must carry that same tag -- including a
//! create, whose `at` is `None` on the wire but still carries a tag once it
//! has passed through this binding once; see `binding/generation.rs`'s
//! `tag_presence`/`untag_presence` for why a bare `None` cannot be trusted
//! here. See `binding/delegate.rs` and `binding/generation.rs` for the rest
//! of the reasoning -- nothing here repeats it, only its shape.

use crate::binding::holding::outliving;
use crate::binding::{generation, PlatformDesiredState};
use crate::{DataSourceDeclaration, DataSourceState, DataSourcesRead, DesiredRevision, DesiredStateError};

#[async_trait::async_trait]
impl DataSourceState for PlatformDesiredState {
    async fn read_data_sources(&self, environment: &str) -> Result<DataSourcesRead, DesiredStateError> {
        let live = self.held().await;
        let repository = live.data_source_repository()?;
        let read_at = live.generation();
        let environment = environment.to_owned();

        let mut read = outliving(
            live,
            async move { repository.read_data_sources(&environment).await },
        )
        .await?;

        // Always Some once tagged, even when the adapter found no file --
        // that absence is generation-tagged too, so a create decided here
        // carries the same proof of freshness a replace already does.
        read.revision = Some(generation::tag_presence(read_at, read.revision.as_ref()));

        Ok(read)
    }

    async fn write_data_sources(
        &self,
        environment: &str,
        declarations: &[DataSourceDeclaration],
        at: Option<&DesiredRevision>,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        let live = self.held().await;
        let repository = live.data_source_repository()?;

        // A bare `None` cannot be trusted: this binding never hands one
        // back (see read_data_sources above), so one arriving here carries
        // no generation to check and is refused exactly as an untagged
        // revision is on a components write.
        let at = generation::untag_presence(live.generation(), at.ok_or(DesiredStateError::Conflict)?)?;

        let (environment, declarations, message) =
            (environment.to_owned(), declarations.to_vec(), message.to_owned());

        outliving(live, async move {
            repository
                .write_data_sources(&environment, &declarations, at.as_ref(), &message)
                .await
        })
        .await
    }
}
