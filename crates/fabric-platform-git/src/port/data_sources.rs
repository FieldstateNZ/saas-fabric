//! Implementing the declared-data-sources port over the platform repository.
//!
//! The same budget contract `port.rs` states for `DesiredState` applies here
//! too: every method runs inside `within_budget`, because the platform
//! binding holds a lock across this call regardless of which port an
//! operation belongs to.

pub(crate) mod document;
mod header;
pub(crate) mod read;
pub(crate) mod write;

use fabric_platform_management::{
    DataSourceDeclaration, DataSourceState, DataSourcesRead, DesiredRevision, DesiredStateError,
};

use crate::host::PlatformGitRepository;

#[async_trait::async_trait]
impl DataSourceState for PlatformGitRepository {
    async fn read_data_sources(&self, environment: &str) -> Result<DataSourcesRead, DesiredStateError> {
        self.within_budget(async {
            let read = self.read_data_sources_file(environment).await?;

            Ok(match read.file {
                None => DataSourcesRead {
                    revision: None,
                    declarations: Vec::new(),
                },
                Some((stored, document)) => DataSourcesRead {
                    revision: Some(DesiredRevision::new(stored.revision.as_str())),
                    declarations: document.into_declarations(),
                },
            })
        })
        .await
    }

    async fn write_data_sources(
        &self,
        environment: &str,
        declarations: &[DataSourceDeclaration],
        at: Option<&DesiredRevision>,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.within_budget(async {
            self.write_data_sources_file(environment, declarations, at, message)
                .await?;

            Ok(())
        })
        .await
    }
}
