//! The catalogue document's outer shape: what identifies it, and how its
//! `spec` carries the catalogue itself.

use serde::{Deserialize, Serialize};
use serde_norway::Value;

use super::Catalogue;
use crate::DesiredStateError;

/// The `apiVersion` every catalogue document this model writes carries.
///
/// Checked on read, before anything about the catalogue itself is trusted —
/// see [`check_document_kind`] — the same discipline the client document
/// applies to its own envelope (`document::schema`), and for the same
/// reason: the repository holds more than one kind of document, and a reader
/// that guessed which one it had would eventually guess wrong.
pub const API_VERSION: &str = "fabric.fieldstate.nz/v1";

/// The `kind` every catalogue document this model writes carries.
///
/// `Catalogue`, not `Client`: the two are siblings in the same repository —
/// one document per client, plus this one — and nothing about their shapes
/// overlaps.
pub const KIND: &str = "Catalogue";

/// The `apiVersion`/[`KIND`] pair as one string, for the message a rejected
/// document produces.
const EXPECTED_DOCUMENT: &str = "fabric.fieldstate.nz/v1/Catalogue";

/// The document exactly as it is written and read: an envelope naming what it
/// is, wrapped around the catalogue itself.
///
/// # Why the envelope is storage-only
///
/// [`StoredCatalogue`](super::StoredCatalogue) — what the HTTP API
/// serialises — carries the catalogue body directly, with no `apiVersion` or
/// `kind` alongside it; a caller that asked for the catalogue already knows
/// what it asked for. The envelope exists so that `fabric-catalogue.yaml`
/// itself says what it is, the way every other desired-state document in this
/// repository already does.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Envelope {
    /// Identifies the schema this document is written against.
    pub(super) api_version: String,

    /// Identifies this as a catalogue document, not a client document.
    pub(super) kind: String,

    /// The catalogue itself.
    pub(super) spec: Catalogue,
}

impl Envelope {
    /// Wraps a catalogue for storage, stamping the current envelope.
    pub(super) fn wrapping(catalogue: Catalogue) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            kind: KIND.to_owned(),
            spec: catalogue,
        }
    }
}

/// Refuses a document that is not a catalogue document of the version this
/// model reads, and says which pair it expected.
///
/// Run *before* the document is deserialised into [`Envelope`], so a document
/// missing its envelope — or carrying the wrong one — is refused as the wrong
/// document rather than as a catalogue missing every field, which would send
/// a reader looking for a bug that is not there.
///
/// # Errors
///
/// Returns [`DesiredStateError::UnknownDocumentKind`], naming the pair this
/// model expects and the pair the document actually carried — or `None` if
/// it carried neither field, which is what a document written before this
/// envelope concept existed looks like.
pub(super) fn check_document_kind(raw: &Value) -> Result<(), DesiredStateError> {
    let api_version = string_at(raw, "apiVersion");
    let kind = string_at(raw, "kind");

    if api_version == Some(API_VERSION) && kind == Some(KIND) {
        return Ok(());
    }

    let found = (api_version.is_some() || kind.is_some()).then(|| {
        format!(
            "{}/{}",
            api_version.unwrap_or("(no apiVersion)"),
            kind.unwrap_or("(no kind)"),
        )
    });

    Err(DesiredStateError::UnknownDocumentKind {
        expected: EXPECTED_DOCUMENT,
        found,
    })
}

/// Reads a top-level string field, if it is present and is a string.
fn string_at<'a>(raw: &'a Value, key: &str) -> Option<&'a str> {
    raw.as_mapping()?.get(key)?.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spelled_out_pair_matches_the_two_constants() {
        assert_eq!(EXPECTED_DOCUMENT, format!("{API_VERSION}/{KIND}"));
    }
}
