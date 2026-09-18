//! Reading an environment's data-sources document, as it stands on the branch.

use crate::host::PlatformGitRepository;
use crate::port::data_sources::document::Document;
use crate::{CommitRevision, PlatformGitError, StoredFile};

/// A data-sources document as read, with everything a write against it
/// needs.
pub(crate) struct ReadDataSources {
    /// The commit it was read at, which a write builds on.
    pub head: CommitRevision,

    /// None when no file exists yet.
    pub file: Option<(StoredFile, Document)>,
}

impl PlatformGitRepository {
    /// Reads an environment's data-sources document and checks it is that
    /// environment's.
    ///
    /// Shared by the port's read and write, so the environment check
    /// happens once -- the same reason `read_manifest` is shared by every
    /// components write.
    ///
    /// # Errors
    ///
    /// `PlatformGitError` if the branch cannot be read, the document does
    /// not parse, or it describes somewhere else.
    pub(crate) async fn read_data_sources_file(
        &self,
        environment: &str,
    ) -> Result<ReadDataSources, PlatformGitError> {
        let head = self.head().await?;
        let path = data_sources_path(environment);

        let stored = match self.read(&path, &head).await {
            Ok(stored) => Some(stored),
            Err(PlatformGitError::NotFound { .. }) => None,
            Err(other) => return Err(other),
        };

        let file = match stored {
            None => None,
            Some(stored) => {
                let document = Document::parse(&stored.text)?;

                if document.environment() != environment {
                    return Err(PlatformGitError::Rejected {
                        detail: format!(
                            "the data sources document declares environment '{}', not '{environment}'",
                            document.environment()
                        ),
                    });
                }

                Some((stored, document))
            }
        };

        Ok(ReadDataSources { head, file })
    }
}

/// Where an environment's data-sources document lives.
///
/// A caller never supplies this: the environment is a key looked up
/// inside the document this returns, never a path segment -- the same
/// rule `manifest_path` states for components.
pub(crate) fn data_sources_path(environment: &str) -> String {
    format!("environments/{environment}/data-sources.yaml")
}
