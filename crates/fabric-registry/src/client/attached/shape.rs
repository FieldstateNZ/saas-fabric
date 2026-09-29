//! Whether verified manifest bytes are a component descriptor's manifest.

use fabric_platform_management::Unusable;

use super::candidate::{Candidate, Checked};
use crate::client::digest::{sha256, Content};
use crate::client::media::{EMPTY_CONFIG, OCI_MANIFEST};
use crate::client::revisions::revisions_in;
use crate::client::wire::Manifest;

/// The annotation a component descriptor names its version in.
const VERSION: &str = "org.opencontainers.image.version";

/// Holds verified manifest bytes to a component descriptor's shape: an OCI
/// image manifest of the family's `artifactType`, the empty config, exactly
/// one layer of that type plus `+json` no larger than the document bound,
/// and `subject` as its subject.
///
/// Another subject is [`Unusable::OtherSubject`]; every other failure,
/// including no `mediaType` or no `subject` at all, is
/// [`Unusable::Malformed`].
pub(super) fn check(subject: &str, content: &Content) -> Candidate {
    let malformed = Candidate::Unusable(Unusable::Malformed);

    let Ok(manifest) = serde_json::from_slice::<Manifest>(&content.bytes) else {
        return malformed;
    };
    let Some(artifact_type) = manifest
        .artifact_type
        .filter(|artifact_type| fabric_component::family_version(artifact_type).is_some())
    else {
        return malformed;
    };
    if manifest.media_type.as_deref() != Some(OCI_MANIFEST)
        || manifest.config.and_then(|config| config.media_type).as_deref() != Some(EMPTY_CONFIG)
    {
        return malformed;
    }

    let layer_type = format!("{artifact_type}+json");
    let layer = match manifest.layers.as_deref() {
        Some([layer]) if layer.media_type.as_deref() == Some(layer_type.as_str()) => layer,
        _ => return malformed,
    };
    let within =
        |size: u64| usize::try_from(size).is_ok_and(|size| size <= fabric_component::MAX_DOCUMENT_BYTES);
    let Some(layer_size) = layer.size.filter(|size| within(*size)) else {
        return malformed;
    };
    if sha256(&layer.digest, "a component descriptor's layer").is_err() {
        return malformed;
    }

    match manifest.subject {
        None => return malformed,
        Some(named) if named.digest != subject => return Candidate::Unusable(Unusable::OtherSubject),
        Some(_) => {}
    }

    // The revision is read by the one rule every image's is read by --
    // trimmed, and empty is none -- so the same value is judged the same
    // wherever it was stamped.
    let revision = revisions_in(manifest.annotations.as_ref()).into_iter().next();
    let version = manifest
        .annotations
        .as_ref()
        .and_then(|values| values.get(VERSION))
        .cloned();

    Candidate::Usable(Checked {
        digest: content.digest.clone(),
        artifact_type,
        layer_digest: layer.digest.clone(),
        layer_size,
        revision,
        version,
    })
}
