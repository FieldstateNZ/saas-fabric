//! The placements document, and the header it was found under.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md), for
//! the same reason `port::data_sources::document` is: one cohesive
//! wire-format type -- the envelope, the schema-version probe, and the
//! header-preserving Document -- together with the parse/render impls that
//! exist only because this shape does.

#[cfg(test)]
#[path = "document_tests.rs"]
mod document_tests;

use fabric_platform_management::PlacementRecord;

use crate::port::placements::header::header_of;
use crate::PlatformGitError;

/// The schema this crate is written against.
///
/// A document declaring anything else is refused rather than half
/// understood, the same rule `data_sources::document::SCHEMA_VERSION`
/// enforces.
const SCHEMA_VERSION: u32 = 1;

/// A placements document, and the header it was found under.
///
/// Mirrors `data_sources::document::Document`: the header is captured
/// verbatim and written back unchanged, so a hand edit under it survives
/// as values and not as formatting.
pub(crate) struct Document {
    header: String,
    environment: String,
    placements: Vec<PlacementRecord>,
}

/// The envelope's own fields, camelCase like `data-sources.yaml`. Each
/// entry is `PlacementRecord` itself, spelled the wire's own `snake_case` --
/// there is only one declaration of that shape.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    environment: String,
    #[serde(default)]
    placements: Vec<PlacementRecord>,
}

/// Just enough of a document to know its schema, mirrors
/// `data_sources::document::Versioned`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Versioned {
    schema_version: u32,
}

impl Document {
    /// Builds a fresh document, sorted by (tenant, logical) so an
    /// unrelated edit produces no diff.
    pub(crate) fn new(header: String, environment: &str, placements: &[PlacementRecord]) -> Self {
        let mut sorted = placements.to_vec();
        sort_by_tenant_and_logical(&mut sorted);

        Self {
            header,
            environment: environment.to_owned(),
            placements: sorted,
        }
    }

    /// Parses a placements document, keeping its header.
    ///
    /// # Errors
    ///
    /// Returns `PlatformGitError::Rejected` if the document does not
    /// parse, or declares a schema version this crate was not written
    /// against.
    pub(crate) fn parse(text: &str) -> Result<Self, PlatformGitError> {
        let declared: Versioned =
            serde_norway::from_str(text).map_err(|error| PlatformGitError::Rejected {
                detail: format!("the placements document could not be read: {error}"),
            })?;

        if declared.schema_version != SCHEMA_VERSION {
            return Err(PlatformGitError::Rejected {
                detail: format!(
                    "the placements document declares schemaVersion {}, and this reads {SCHEMA_VERSION}",
                    declared.schema_version
                ),
            });
        }

        let envelope: Envelope =
            serde_norway::from_str(text).map_err(|error| PlatformGitError::Rejected {
                detail: format!("the placements document could not be read: {error}"),
            })?;

        let mut placements = envelope.placements;
        sort_by_tenant_and_logical(&mut placements);

        Ok(Self {
            header: header_of(text),
            environment: envelope.environment,
            placements,
        })
    }

    /// Renders the document back, under the header it came with (or the
    /// fixed create header, for a document that does not exist yet).
    ///
    /// # Errors
    ///
    /// Returns `PlatformGitError::Unavailable` if the placements cannot be
    /// serialised, which would mean a value this crate constructed is not
    /// representable -- a defect rather than a condition.
    pub(crate) fn render(&self) -> Result<String, PlatformGitError> {
        let envelope = Envelope {
            schema_version: SCHEMA_VERSION,
            environment: self.environment.clone(),
            placements: self.placements.clone(),
        };

        let body = serde_norway::to_string(&envelope).map_err(|error| PlatformGitError::Unavailable {
            detail: format!("the placements document could not be written: {error}"),
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

    /// The placements, sorted by (tenant, logical), consuming the document.
    pub(crate) fn into_placements(self) -> Vec<PlacementRecord> {
        self.placements
    }
}

/// The sort ADR 0023 part 2 states: by (tenant, logical), so the file
/// reads the same way an operator would look a tenant up in it.
fn sort_by_tenant_and_logical(placements: &mut [PlacementRecord]) {
    placements.sort_by(|left, right| (&left.tenant, &left.logical).cmp(&(&right.tenant, &right.logical)));
}
