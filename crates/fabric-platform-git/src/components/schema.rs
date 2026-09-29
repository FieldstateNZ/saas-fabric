//! What a manifest's own schema version allows, beyond what parsing checks.

use crate::components::{Artifact, Manifest, DESCRIBED_SCHEMA_VERSION, SCHEMA_VERSIONS};
use crate::PlatformGitError;

/// Refuses what a manifest may not say at the version it declares, and a
/// described component whose primary is not one of its images.
///
/// # Why a schema 2 file may not hold a described component
///
/// Every build that predates schema 3 refuses the file by its version,
/// naming it, rather than failing on an unknown artifact type (ADR 0026
/// section 9). A `described` artifact under `schemaVersion: 2` would take
/// that away: an older build would read the version, accept it, and then
/// fail on the field — the one diagnostic reading the version first exists
/// to avoid. So the type requires the version, and a file that has one
/// without the other is refused here, naming both.
///
/// # Why the primary must be one of the images
///
/// The primary is the role whose image carries the component descriptor,
/// and whose repository lists the versions there are. One the component does
/// not pin has no repository to list and no image to repin, so nothing could
/// ever be discovered or written for it.
///
/// # Errors
///
/// [`PlatformGitError::Rejected`] for the first component that breaks
/// either rule, by name.
pub(super) fn check(manifest: &Manifest) -> Result<(), PlatformGitError> {
    for (name, component) in &manifest.components {
        // Every kind named: a new one must decide whether a schema version
        // gates it, rather than skip this check by default.
        let (primary, images) = match &component.artifact {
            Artifact::Oci { .. } | Artifact::Helm { .. } => continue,
            Artifact::Described { primary, images, .. } => (primary, images),
        };

        if manifest.schema_version < DESCRIBED_SCHEMA_VERSION {
            return Err(PlatformGitError::Rejected {
                detail: format!(
                    "{name} is a described component, which needs schemaVersion \
                     {DESCRIBED_SCHEMA_VERSION}; the components manifest declares schemaVersion {}",
                    manifest.schema_version
                ),
            });
        }

        if !images.contains_key(primary) {
            return Err(PlatformGitError::Rejected {
                detail: format!("{name}'s primary '{primary}' is not one of its images"),
            });
        }
    }

    Ok(())
}

/// The versions this reads, for a refusal: `2 or 3`.
pub(super) fn readable() -> String {
    let versions: Vec<String> = SCHEMA_VERSIONS.iter().map(u32::to_string).collect();
    versions.join(" or ")
}
