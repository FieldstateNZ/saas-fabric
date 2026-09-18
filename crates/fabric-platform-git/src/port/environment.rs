//! Writing an environment's data sources and placements in one commit.
//!
//! ADR 0023 part 2 (B4): `place` and `remove` each read one document and
//! write the other, so two single-document compare-and-swaps could
//! interleave and leave a placement naming a source nothing declares. This
//! is the fix -- one `update_files_atomically` call carrying both
//! `FileChange`s, each with its own expected revision, so either both land
//! or neither does.

use fabric_platform_management::{DesiredStateError, EnvironmentWrite, PlatformRepository};

use crate::host::PlatformGitRepository;
use crate::port::data_sources::document::Document as DataSourcesDocument;
use crate::port::data_sources::read::data_sources_path;
use crate::port::data_sources::write::CREATE_HEADER as DATA_SOURCES_CREATE_HEADER;
use crate::port::placements::document::Document as PlacementsDocument;
use crate::port::placements::read::placements_path;
use crate::port::placements::write::CREATE_HEADER as PLACEMENTS_CREATE_HEADER;
use crate::{CommitRevision, FileChange, PlatformGitError, StoredFile};

#[async_trait::async_trait]
impl PlatformRepository for PlatformGitRepository {
    async fn write_environment(
        &self,
        environment: &str,
        write: EnvironmentWrite<'_>,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.within_budget(async {
            self.write_environment_files(environment, write, message).await?;

            Ok(())
        })
        .await
    }
}

impl PlatformGitRepository {
    /// Builds both `FileChange`s and writes them in one commit.
    ///
    /// The two documents are read independently (each carries its own
    /// `head`), which is safe: `update_files_atomically` does not need
    /// both reads to share a head, only each file's own expected revision
    /// to be honest. A commit that lands between the two reads costs a
    /// retry inside that call, exactly as it would for either document
    /// alone.
    async fn write_environment_files(
        &self,
        environment: &str,
        write: EnvironmentWrite<'_>,
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        let data_sources_read = self.read_data_sources_file(environment).await?;
        let placements_read = self.read_placements_file(environment).await?;

        let data_sources_change = file_change(
            data_sources_path(environment),
            data_sources_read
                .file
                .map(|(stored, document)| (stored, document.header())),
            write.data_sources.1,
            DATA_SOURCES_CREATE_HEADER,
            |header| DataSourcesDocument::new(header, environment, write.data_sources.0).render(),
        )?;

        let placements_change = file_change(
            placements_path(environment),
            placements_read
                .file
                .map(|(stored, document)| (stored, document.header())),
            write.placements.1,
            PLACEMENTS_CREATE_HEADER,
            |header| PlacementsDocument::new(header, environment, write.placements.0).render(),
        )?;

        let changes = vec![data_sources_change, placements_change];

        self.update_files_atomically(&data_sources_read.head, &changes, message)
            .await
    }
}

/// Builds one document's `FileChange`: the same header-preserving,
/// compare-and-swap logic `write_data_sources_file`/`write_placements_file`
/// each carry for their own single document, shared here because
/// `write_environment` needs it twice in the one call.
fn file_change(
    path: String,
    current: Option<(StoredFile, String)>,
    at: Option<&fabric_platform_management::DesiredRevision>,
    create_header: &str,
    render: impl FnOnce(String) -> Result<String, PlatformGitError>,
) -> Result<FileChange, PlatformGitError> {
    let header = match (&current, at) {
        (None, None) => create_header.to_owned(),
        (None, Some(_)) | (Some(_), None) => return Err(PlatformGitError::Conflict { path }),
        (Some((stored, header)), Some(at)) => {
            if stored.revision.as_str() != at.as_str() {
                return Err(PlatformGitError::Conflict { path });
            }
            header.clone()
        }
    };

    let expected = current.map(|(stored, _)| stored.revision);
    let text = render(header)?;

    Ok(FileChange { path, text, expected })
}
