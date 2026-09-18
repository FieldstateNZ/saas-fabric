//! Delegating the two-document write to whatever is bound.
//!
//! The same shape as `binding/data_sources.rs` and `binding/placements.rs`,
//! which are the same shape as `binding/delegate.rs`: the operation runs in
//! a task that owns the read guard until the delegated call has an
//! outcome. What is different here is that there are two revisions to
//! untag instead of one -- both against the *same* generation, read once
//! under the one guard, so a rebind between the read either half was
//! decided from and this write is refused for both at once rather than
//! leaving one document checked and the other trusted.

use crate::binding::holding::outliving;
use crate::binding::{generation, PlatformDesiredState};
use crate::{DesiredStateError, EnvironmentWrite, PlatformRepository};

#[async_trait::async_trait]
impl PlatformRepository for PlatformDesiredState {
    async fn write_environment(
        &self,
        environment: &str,
        write: EnvironmentWrite<'_>,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        let live = self.held().await;
        let repository = live.platform_repository()?;

        // Both untagged against the one generation this guard was taken
        // under -- see `binding/generation.rs` for why a bare `None`
        // carries nothing to check and is refused the same as a mismatch.
        let data_sources_at = generation::untag_presence(
            live.generation(),
            write.data_sources.1.ok_or(DesiredStateError::Conflict)?,
        )?;
        let placements_at = generation::untag_presence(
            live.generation(),
            write.placements.1.ok_or(DesiredStateError::Conflict)?,
        )?;

        let (environment, data_sources, placements, message) = (
            environment.to_owned(),
            write.data_sources.0.to_vec(),
            write.placements.0.to_vec(),
            message.to_owned(),
        );

        outliving(live, async move {
            repository
                .write_environment(
                    &environment,
                    EnvironmentWrite {
                        data_sources: (&data_sources, data_sources_at.as_ref()),
                        placements: (&placements, placements_at.as_ref()),
                    },
                    &message,
                )
                .await
        })
        .await
    }
}
