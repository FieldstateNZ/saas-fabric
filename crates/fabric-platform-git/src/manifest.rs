//! Reading an environment's manifest, and where it lives.

use crate::components::Document;
use crate::host::PlatformGitRepository;
use crate::{CommitRevision, PlatformGitError, StoredFile};

/// A manifest as read, with everything a write against it needs.
pub(crate) struct ReadManifest {
    /// The commit it was read at, which every write is built on.
    pub head: CommitRevision,

    /// The file, carrying the revision that makes the write conditional.
    pub stored: StoredFile,

    /// The parsed document.
    pub document: Document,
}

impl PlatformGitRepository {
    /// Reads an environment's manifest and checks it is that environment's.
    ///
    /// Shared by every write, so the environment check happens once rather
    /// than once per operation — the failure it prevents is a manifest moved
    /// or copied between directories, where writing what the *path* said would
    /// pin the wrong environment while looking entirely correct.
    pub(crate) async fn read_manifest(&self, environment: &str) -> Result<ReadManifest, PlatformGitError> {
        let head = self.head().await?;
        let path = manifest_path(environment);

        let stored = self.read(&path, &head).await?;
        let document = Document::parse(&stored.text)?;

        if document.manifest.environment != environment {
            return Err(PlatformGitError::Rejected {
                detail: format!(
                    "{path} describes '{}', not '{environment}'",
                    document.manifest.environment
                ),
            });
        }

        Ok(ReadManifest {
            head,
            stored,
            document,
        })
    }
}

/// Where an environment's manifest lives.
///
/// The one place this is spelled out. A caller never supplies it: the
/// specification is explicit that a request may not name a repository file,
/// and an environment name is not a path.
///
/// A *component* name is different, and is deliberately not here. It is a key
/// looked up inside the document this returns, never a path segment, a
/// registry location, or any other locator — so an operator may name one and
/// still cannot name a file.
pub(crate) fn manifest_path(environment: &str) -> String {
    format!("environments/{environment}/components.yaml")
}
