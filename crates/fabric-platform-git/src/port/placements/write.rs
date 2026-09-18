//! Replacing an environment's whole placements document, in one commit.

#[cfg(test)]
#[path = "write_tests.rs"]
mod write_tests;

use fabric_platform_management::{DesiredRevision, PlacementRecord};

use crate::host::PlatformGitRepository;
use crate::port::placements::document::Document;
use crate::port::placements::read::placements_path;
use crate::{CommitRevision, FileChange, PlatformGitError};

/// The header written the first time an environment places a tenant. Every
/// rewrite after the first preserves whatever header is actually on the
/// branch instead, hand edits included -- the same rule
/// `data_sources::write::CREATE_HEADER` states.
pub(crate) const CREATE_HEADER: &str = r"# Which data source each tenant's intent is placed on, and how it is isolated.
#
# Machine-managed by SaaS Fabric Platform Management. Written by Fabric when
# a tenant is placed, in a deterministic layout, so a hand edit survives as
# values and not as formatting. Editing it by hand is the break-glass path
# and is expected to keep working.
#
# This is the record ADR 0023 part 2 makes the fact: publication copies it
# into a tenant's binding rather than recomputing it. Nothing here is a
# credential.
#
# See environments/README.md for the complete contract.
---
";

impl PlatformGitRepository {
    /// Replaces an environment's recorded placements with exactly this
    /// list, in one commit.
    ///
    /// `at` is `None` to create -- refused if the file already exists --
    /// or `Some(revision)` to replace -- refused unless the file is still
    /// at that revision. Both refusals answer `PlatformGitError::Conflict`,
    /// the same compare-and-swap `write_data_sources_file` uses.
    ///
    /// # Errors
    ///
    /// `PlatformGitError::Conflict` on either mismatch above, and the
    /// transport variants otherwise.
    pub(crate) async fn write_placements_file(
        &self,
        environment: &str,
        placements: &[PlacementRecord],
        at: Option<&DesiredRevision>,
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        let read = self.read_placements_file(environment).await?;
        let path = placements_path(environment);

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
        let document = Document::new(header, environment, placements);
        let text = document.render()?;

        let changes = vec![FileChange { path, text, expected }];

        self.update_files_atomically(&read.head, &changes, message).await
    }
}
