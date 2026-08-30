//! Reading a stored document into the typed view.

use serde_norway::Value;

use crate::document::schema::{self, DocumentShape};
use crate::{Client, ClientDocument, DesiredStateError};

/// The inclusive maximum length of a display name.
const MAX_DISPLAY_NAME: usize = 128;

/// How much of an unexpected `apiVersion/kind` pair is quoted back.
const MAX_QUOTED_KIND: usize = 80;

/// Parses a stored document, checking everything before it is trusted.
///
/// The order is deliberate. The document is identified *before* it is
/// deserialised into the reading shape, so a `kind: Tenant` document is refused
/// as the wrong kind rather than as a mysteriously incomplete client — the
/// first message points an operator at the actual problem, the second sends
/// them looking for a field the document was never supposed to have.
pub(super) fn parse(text: &str) -> Result<ClientDocument, DesiredStateError> {
    let raw: Value = serde_norway::from_str(text).map_err(|error| malformed(&error))?;

    check_document_kind(&raw)?;

    let shape: DocumentShape = serde_norway::from_value(raw.clone()).map_err(|error| malformed(&error))?;

    let display_name = check_display_name(shape.spec.display_name)?;
    shape.spec.identity.validate()?;
    shape.spec.authorization.validate()?;

    let client = Client {
        id: shape.metadata.name,
        display_name,
        hosts: shape.spec.hosts,
        identity: shape.spec.identity,
        authorization: shape.spec.authorization,
        secrets: shape.spec.secrets,
    };

    Ok(ClientDocument::from_parts(raw, client))
}

/// Refuses a document that is not a client document of a version this model
/// writes.
fn check_document_kind(raw: &Value) -> Result<(), DesiredStateError> {
    let api_version = string_at(raw, "apiVersion");
    let kind = string_at(raw, "kind");

    if api_version == Some(schema::API_VERSION) && kind == Some(schema::KIND) {
        return Ok(());
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

/// Bounds a display name and refuses characters that would corrupt whatever
/// renders it.
///
/// A display name is the one free-text field in the document. It reaches log
/// lines, an operator's screen, and a Git commit message, so a newline or a
/// control character in it is not a cosmetic problem — it is how one audit
/// record becomes two.
fn check_display_name(value: String) -> Result<String, DesiredStateError> {
    if value.trim().is_empty() {
        return Err(DesiredStateError::MissingField {
            field: "spec.displayName",
        });
    }

    if value.len() > MAX_DISPLAY_NAME {
        return Err(DesiredStateError::InvalidField {
            field: "spec.displayName",
            detail: format!(
                "must be at most {MAX_DISPLAY_NAME} characters, got {}",
                value.len()
            ),
        });
    }

    if value.chars().any(char::is_control) {
        return Err(DesiredStateError::InvalidField {
            field: "spec.displayName",
            detail: "must not contain control characters".to_owned(),
        });
    }

    Ok(value)
}

/// Turns a serde failure into a domain error.
///
/// The serde message carries the field path it failed at — `spec.identity.
/// roles[0]`, and so on — which is the most useful thing an operator can be
/// told and names nothing an operator should not see.
fn malformed(error: &serde_norway::Error) -> DesiredStateError {
    DesiredStateError::Malformed {
        detail: error.to_string(),
    }
}
