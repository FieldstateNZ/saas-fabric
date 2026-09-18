//! Replacing an environment's whole data-sources document, in one commit.

#[cfg(test)]
#[path = "write_tests.rs"]
mod write_tests;

use fabric_platform_management::{DataSourceDeclaration, DesiredRevision};

use crate::host::PlatformGitRepository;
use crate::port::data_sources::document::Document;
use crate::port::data_sources::read::data_sources_path;
use crate::{CommitRevision, FileChange, PlatformGitError};

/// The header written the first time an environment declares a data
/// source. ADR 0023 part 1 states this text; every rewrite after the
/// first preserves whatever header is actually on the branch instead,
/// hand edits included.
const CREATE_HEADER: &str = r"# What this environment can place a tenant's data on.
#
# Machine-managed by SaaS Fabric Platform Management. Everything below this
# header is written by Fabric in a deterministic layout, so a hand edit
# survives as values and not as formatting. Editing it by hand is the
# break-glass path and is expected to keep working.
#
# Desired state only. The runtime reads what is *published* from this file
# (ADR 0023 in the application repository); nothing here is a credential,
# and a connection is a name the connector holds or a reference to a secret.
#
# See environments/README.md for the complete contract.
---
";

impl PlatformGitRepository {
    /// Replaces an environment's declared data sources with exactly this
    /// list, in one commit.
    ///
    /// at is None to create -- refused if the file already exists -- or
    /// Some(revision) to replace, refused unless the file is still at
    /// that revision. Both refusals answer `PlatformGitError::Conflict`,
    /// the same compare-and-swap `set_component_hold` uses.
    ///
    /// # Errors
    ///
    /// `PlatformGitError::Conflict` on either mismatch above, and the
    /// transport variants otherwise.
    pub(crate) async fn write_data_sources_file(
        &self,
        environment: &str,
        declarations: &[DataSourceDeclaration],
        at: Option<&DesiredRevision>,
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        let read = self.read_data_sources_file(environment).await?;
        let path = data_sources_path(environment);

        let header = match (&read.file, at) {
            (None, None) => CREATE_HEADER.to_owned(),
            (None, Some(_)) | (Some(_), None) => {
                return Err(PlatformGitError::Conflict { path });
            }
            (Some((stored, document)), Some(at)) => {
                if stored.revision.as_str() != at.as_str() {
                    return Err(PlatformGitError::Conflict { path });
                }

                document.header()
            }
        };

        let expected = read.file.as_ref().map(|(stored, _)| stored.revision.clone());
        let document = Document::new(header, environment, declarations);
        let text = document.render()?;

        let changes = vec![FileChange { path, text, expected }];

        self.update_files_atomically(&read.head, &changes, message).await
    }
}
