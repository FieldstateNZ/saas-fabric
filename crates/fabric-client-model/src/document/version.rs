//! Which document this is, checked before anything else is read.

use serde_norway::Value;

use crate::document::schema;
use crate::DesiredStateError;

/// How much of an unexpected `apiVersion/kind` pair is quoted back.
const MAX_QUOTED_KIND: usize = 80;

/// Which of the two schema versions a document declares.
///
/// Two accepted pairs rather than one replaced pair. The repository holds
/// documents nobody is going to migrate on a schedule the platform controls,
/// so `v1` keeps parsing and this is what tells `parse` which reading applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SchemaVersion {
    /// Deprecated, still read, and migrated on the way into the typed view.
    V1,
    /// The version this model writes.
    V2,
}

/// Refuses a document that is not a client document of a version this model
/// reads, and says which version it is.
///
/// Run *before* the document is deserialised into the reading shape, so a
/// `kind: Tenant` document is refused as the wrong kind rather than as a
/// mysteriously incomplete client — the first message points an operator at
/// the actual problem, the second sends them looking for a field the document
/// was never supposed to have.
///
/// # Errors
///
/// Returns [`DesiredStateError::UnknownDocumentKind`], quoting back what the
/// document actually carried, bounded so a hostile file cannot fill a log
/// line with it.
pub(super) fn check_document_kind(raw: &Value) -> Result<SchemaVersion, DesiredStateError> {
    let api_version = string_at(raw, "apiVersion");
    let kind = string_at(raw, "kind");

    if kind == Some(schema::KIND) {
        match api_version {
            Some(schema::API_VERSION_V2) => return Ok(SchemaVersion::V2),
            Some(schema::API_VERSION) => return Ok(SchemaVersion::V1),
            _ => {}
        }
    }

    let found: String = format!(
        "{}/{}",
        api_version.unwrap_or("(no apiVersion)"),
        kind.unwrap_or("(no kind)")
    )
    .chars()
    .take(MAX_QUOTED_KIND)
    .collect();

    Err(DesiredStateError::UnknownDocumentKind {
        expected: schema::EXPECTED_DOCUMENT,
        found,
    })
}

/// Reads a top-level string field, if it is present and is a string.
fn string_at<'a>(raw: &'a Value, key: &str) -> Option<&'a str> {
    raw.as_mapping()?.get(key)?.as_str()
}
