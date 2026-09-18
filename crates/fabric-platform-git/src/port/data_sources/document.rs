//! The data-sources document, and the header it was found under.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md): one
//! cohesive wire-format type -- the envelope, the schema-version probe, and
//! the header-preserving Document -- together with the parse/render impls
//! that exist only because this shape does. Splitting them would separate
//! a document from the format it reads and writes.

#[cfg(test)]
#[path = "document_tests.rs"]
mod document_tests;

use fabric_platform_management::DataSourceDeclaration;

use crate::port::data_sources::header::header_of;
use crate::PlatformGitError;

/// The schema this crate is written against.
///
/// A document declaring anything else is refused rather than half
/// understood, the same rule `components::SCHEMA_VERSION` enforces.
const SCHEMA_VERSION: u32 = 1;

/// A data-sources document, and the header it was found under.
///
/// Mirrors `components::Document`: the header is captured verbatim and
/// written back unchanged, so a hand edit under it survives as values and
/// not as formatting.
pub(crate) struct Document {
    header: String,
    environment: String,
    declarations: Vec<DataSourceDeclaration>,
}

/// The envelope's own fields, camelCase like `components.yaml`. Each entry
/// is `DataSourceDeclaration` itself, spelled the wire's own `snake_case` --
/// there is only one declaration of that shape.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    environment: String,
    #[serde(default)]
    data_sources: Vec<DataSourceDeclaration>,
}

/// Just enough of a document to know its schema, mirrors
/// `components::Versioned`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Versioned {
    schema_version: u32,
}

impl Document {
    /// Builds a fresh document, sorted by id so an unrelated edit produces
    /// no diff.
    pub(crate) fn new(header: String, environment: &str, declarations: &[DataSourceDeclaration]) -> Self {
        let mut sorted = declarations.to_vec();
        sorted.sort_by(|left, right| left.id.cmp(&right.id));

        Self {
            header,
            environment: environment.to_owned(),
            declarations: sorted,
        }
    }

    /// Parses a data-sources document, keeping its header.
    ///
    /// # Errors
    ///
    /// Returns `PlatformGitError::Rejected` if the document does not
    /// parse, or declares a schema version this crate was not written
    /// against.
    pub(crate) fn parse(text: &str) -> Result<Self, PlatformGitError> {
        let declared: Versioned =
            serde_norway::from_str(text).map_err(|error| PlatformGitError::Rejected {
                detail: format!("the data sources document could not be read: {error}"),
            })?;

        if declared.schema_version != SCHEMA_VERSION {
            return Err(PlatformGitError::Rejected {
                detail: format!(
                    "the data sources document declares schemaVersion {}, and this reads {SCHEMA_VERSION}",
                    declared.schema_version
                ),
            });
        }

        let envelope: Envelope =
            serde_norway::from_str(text).map_err(|error| PlatformGitError::Rejected {
                detail: format!("the data sources document could not be read: {error}"),
            })?;

        let mut declarations = envelope.data_sources;
        declarations.sort_by(|left, right| left.id.cmp(&right.id));

        Ok(Self {
            header: header_of(text),
            environment: envelope.environment,
            declarations,
        })
    }

    /// Renders the document back, under the header it came with (or the
    /// fixed create header, for a document that does not exist yet).
    ///
    /// # Errors
    ///
    /// Returns `PlatformGitError::Unavailable` if the declarations cannot
    /// be serialised, which would mean a value this crate constructed is
    /// not representable -- a defect rather than a condition.
    pub(crate) fn render(&self) -> Result<String, PlatformGitError> {
        let envelope = Envelope {
            schema_version: SCHEMA_VERSION,
            environment: self.environment.clone(),
            data_sources: self.declarations.clone(),
        };

        let body = serde_norway::to_string(&envelope).map_err(|error| PlatformGitError::Unavailable {
            detail: format!("the data sources document could not be written: {error}"),
        })?;

        Ok(format!("{}{body}", self.header))
    }

    /// The header this document was read under, or was created with.
    pub(crate) fn header(&self) -> String {
        self.header.clone()
    }

    /// Which environment this document declares itself to describe.
    pub(crate) fn environment(&self) -> &str {
        &self.environment
    }

    /// The declarations, sorted by id, consuming the document.
    pub(crate) fn into_declarations(self) -> Vec<DataSourceDeclaration> {
        self.declarations
    }
}
