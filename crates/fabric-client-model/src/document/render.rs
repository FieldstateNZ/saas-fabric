//! Writing the typed view back into the stored document.

use serde_norway::Value;

use crate::document::{parse, schema};
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
///
/// # Why an edit migrates a `v1` document
///
/// That same second parse is what forces it. `spec.identity` is written in the
/// `v2` shape, because that is the only shape the typed view has, and a `v2`
/// identity block under a `v1` `apiVersion` fails the re-parse. So the edit
/// either migrates the document or it fails, and failing would make `v1`
/// documents read-only for no reason anybody asked for.
///
/// It is a migration performed by an *edit*, never a reinterpretation: the
/// operator asked for a change and receives a document that says `v2` at the
/// top. A file nobody has edited stays `v1`, and no sweep migrates anything —
/// the control plane rewrites only documents an operator has actually changed.
pub(super) fn with_identity(
    document: &ClientDocument,
    identity: IdentityConfiguration,
) -> Result<ClientDocument, DesiredStateError> {
    identity.validate()?;

    let mut raw = document.raw().clone();

    // `insert` on an ordered mapping replaces in place, so `apiVersion` keeps
    // the position it was written in and the diff an operator reviews shows a
    // one-line version change rather than a moved key.
    raw.as_mapping_mut()
        .ok_or(DesiredStateError::MissingField { field: "apiVersion" })?
        .insert(
            Value::String("apiVersion".to_owned()),
            Value::String(schema::API_VERSION_V2.to_owned()),
        );

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
