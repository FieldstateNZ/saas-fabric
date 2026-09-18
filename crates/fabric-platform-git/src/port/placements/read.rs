//! Reading an environment's placements document, as it stands on the branch.

use crate::host::PlatformGitRepository;
use crate::port::placements::document::Document;
use crate::{CommitRevision, PlatformGitError, StoredFile};

/// A placements document as read, with everything a write against it needs.
pub(crate) struct ReadPlacements {
    /// The commit it was read at, which a write builds on.
    pub head: CommitRevision,

    /// `None` when no file exists yet.
    pub file: Option<(StoredFile, Document)>,
}

impl PlatformGitRepository {
    /// Reads an environment's placements document and checks it is that
    /// environment's.
    ///
    /// Shared by the port's read and write, the same reason
    /// `read_data_sources_file` is.
    ///
    /// # Errors
    ///
    /// `PlatformGitError` if the branch cannot be read, the document does
    /// not parse, or it describes somewhere else.
    pub(crate) async fn read_placements_file(
        &self,
        environment: &str,
    ) -> Result<ReadPlacements, PlatformGitError> {
        let head = self.head().await?;
        let path = placements_path(environment);

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
                            "the placements document declares environment '{}', not '{environment}'",
                            document.environment()
                        ),
                    });
                }

                Some((stored, document))
            }
        };

        Ok(ReadPlacements { head, file })
    }
}

/// Where an environment's placements document lives.
///
/// A caller never supplies this: the environment is a key looked up inside
/// the document this returns, never a path segment -- the same rule
/// `data_sources_path` states for data sources.
pub(crate) fn placements_path(environment: &str) -> String {
    format!("environments/{environment}/placements.yaml")
}
