//! Writing the typed view back into the stored document.

use serde_norway::Value;

use crate::document::parse;
use crate::{ClientDocument, DesiredStateError, IdentityConfiguration};

/// Serialises a document back to YAML.
pub(super) fn render(raw: &Value) -> Result<String, DesiredStateError> {
    serde_norway::to_string(raw).map_err(|error| DesiredStateError::Malformed {
        detail: error.to_string(),
    })
}

/// Replaces `spec.identity` and leaves the rest of the document alone.
///
/// # Why the result is re-parsed
///
/// The obvious implementation validates the incoming configuration, writes it
/// in, and returns. This one writes it in and then parses the whole document
/// again from the merged value.
///
/// That second parse is the guarantee: it means a `ClientDocument` handed to a
/// repository has been read by exactly the code that will read it back, so
/// there is no path that produces a document this model would later refuse.
/// The cost is one serialisation round trip per edit, on a path that is
/// already making a network call to Git.
pub(super) fn with_identity(
    document: &ClientDocument,
    identity: IdentityConfiguration,
) -> Result<ClientDocument, DesiredStateError> {
    identity.validate()?;

    let mut raw = document.raw().clone();
    let spec = raw
        .as_mapping_mut()
        .and_then(|mapping| mapping.get_mut("spec"))
        .and_then(Value::as_mapping_mut)
        .ok_or(DesiredStateError::MissingField { field: "spec" })?;

    let encoded = serde_norway::to_value(identity).map_err(|error| DesiredStateError::Malformed {
        detail: error.to_string(),
    })?;

    // `insert` on an ordered mapping replaces in place, so an existing
    // `identity` key keeps its position and the diff stays local to the
    // section that changed.
    spec.insert(Value::String("identity".to_owned()), encoded);

    let text = render(&raw)?;
    parse::parse(&text)
}
