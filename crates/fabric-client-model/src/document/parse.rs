//! Reading a stored document into the typed view.

use serde_norway::Value;

use crate::document::migration;
use crate::document::schema::DocumentShape;
use crate::document::version::{self, SchemaVersion};
use crate::{Client, ClientDocument, DesiredStateError};

/// The inclusive maximum length of a display name.
const MAX_DISPLAY_NAME: usize = 128;

/// Parses a stored document, checking everything before it is trusted.
///
/// The order is deliberate, and every step before the typed deserialisation is
/// there so the message an operator gets names something they can find in
/// their own file. The document is identified first (`version`), then read
/// through the schema version it declares (`migration`), and only then handed
/// to serde — whose message would otherwise be about a field the document was
/// never supposed to have.
///
/// What the migrator rewrites is a *copy*. The document kept for rendering is
/// the one that was stored, so a `v1` file nobody has edited stays `v1` and
/// stays labelled `v1`.
pub(super) fn parse(text: &str) -> Result<ClientDocument, DesiredStateError> {
    let raw: Value = serde_norway::from_str(text).map_err(|error| malformed(&error))?;

    let reading = match version::check_document_kind(&raw)? {
        SchemaVersion::V1 => migration::to_v2(raw.clone())?,
        SchemaVersion::V2 => {
            migration::reject_replaced_field(&raw)?;
            raw.clone()
        }
    };

    let shape: DocumentShape = serde_norway::from_value(reading).map_err(|error| malformed(&error))?;

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
