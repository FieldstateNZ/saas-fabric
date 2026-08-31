//! Reading and rewriting the manifest, keeping the words the repository wrote.

#[cfg(test)]
#[path = "document_tests.rs"]
mod document_tests;

use crate::components::{Manifest, SCHEMA_VERSION};
use crate::PlatformGitError;

/// A manifest, and the header it was found under.
///
/// # Why the header is carried rather than owned
///
/// The manifest is machine-managed: it is rewritten whole, so the formatting
/// below the header is this crate's and a hand edit survives as values rather
/// than as layout. The *header* is different. It is where the platform
/// repository explains what the file is and who may write it, and a Fabric
/// release replacing that with its own prose would be Fabric editorialising in
/// somebody else's repository.
///
/// So everything from the top of the file down to the first line of content is
/// captured verbatim and written back unchanged.
pub(crate) struct Document {
    /// Comment and separator lines from the top of the file.
    header: String,

    /// The parsed manifest.
    pub(crate) manifest: Manifest,
}

impl Document {
    /// Parses a manifest, keeping its header.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformGitError::Rejected`] if the document does not parse,
    /// or declares a schema version this crate was not written against.
    pub(crate) fn parse(text: &str) -> Result<Self, PlatformGitError> {
        let manifest: Manifest =
            serde_norway::from_str(text).map_err(|error| PlatformGitError::Rejected {
                detail: format!("the components manifest could not be read: {error}"),
            })?;

        if manifest.schema_version != SCHEMA_VERSION {
            return Err(PlatformGitError::Rejected {
                detail: format!(
                    "the components manifest declares schemaVersion {}, and this reads {SCHEMA_VERSION}",
                    manifest.schema_version
                ),
            });
        }

        Ok(Self {
            header: header_of(text),
            manifest,
        })
    }

    /// Renders the manifest back, under the header it came with.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformGitError::Unavailable`] if the manifest cannot be
    /// serialised, which would mean a value this crate constructed is not
    /// representable — a defect rather than a condition.
    pub(crate) fn render(&self) -> Result<String, PlatformGitError> {
        let body =
            serde_norway::to_string(&self.manifest).map_err(|error| PlatformGitError::Unavailable {
                detail: format!("the components manifest could not be written: {error}"),
            })?;

        Ok(format!("{}{body}", self.header))
    }
}

/// The leading comment block, including any document separator.
///
/// Stops at the first line that is content. A blank line inside the comment
/// block is kept, because a header written in paragraphs should stay in
/// paragraphs; a blank line *after* it is not, because the renderer supplies
/// its own layout below.
fn header_of(text: &str) -> String {
    let mut header = String::new();
    let mut pending_blanks = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            pending_blanks.push_str(line);
            pending_blanks.push('\n');
            continue;
        }

        if trimmed.starts_with('#') || trimmed == "---" {
            header.push_str(&pending_blanks);
            pending_blanks.clear();
            header.push_str(line);
            header.push('\n');
            continue;
        }

        break;
    }

    header
}
