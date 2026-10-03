//! What a Kustomize image pin writes for either image kind, and the refusal
//! for a renderer that does not match its artifact.

use std::collections::BTreeMap;

use crate::components::{repin, Artifact, ImagePin, Pin};
use crate::desired::ComponentVersion;
use crate::PlatformGitError;

/// Repins one role's image, shared by both image kinds.
pub(super) fn repin_role(
    text: &str,
    path: &str,
    component: &str,
    images: &BTreeMap<String, ImagePin>,
    role: &str,
    unit: &ComponentVersion,
) -> Result<Option<String>, PlatformGitError> {
    // A pin naming an image the component does not publish is the manifest
    // disagreeing with itself, and repinning anyway would write whichever
    // entry happened to match.
    let image = images.get(role).ok_or_else(|| PlatformGitError::Rejected {
        detail: format!("{path} pins '{role}', which {component} does not publish"),
    })?;

    let Some(offered) = unit.images.get(role) else {
        return Ok(None);
    };

    Ok(Some(repin(
        text,
        &image.repository,
        &unit.version,
        &offered.digest,
    )?))
}

/// A renderer that does not match the artifact: the manifest disagreeing
/// with itself.
pub(super) fn unrenderable(path: &str, component: &str, pin: &Pin, artifact: &Artifact) -> PlatformGitError {
    PlatformGitError::Rejected {
        detail: format!(
            "{path} renders {} for {component}, which is published as {}",
            pin.describe(),
            artifact.describe()
        ),
    }
}
