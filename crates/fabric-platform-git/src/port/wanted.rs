//! Turning a release unit into what a write speaks.

use fabric_platform_management::ReleaseUnit;

use crate::desired::{ComponentVersion, ImageDigest};

/// A release unit, in the vocabulary the write speaks.
///
/// Whole-unit in, whole-unit out: the version, the source commit and every
/// image digest are carried across together, so there is no shape here in
/// which a caller could move one image or supply a digest of their own.
pub(super) fn wanted_from(unit: &ReleaseUnit) -> ComponentVersion {
    ComponentVersion {
        version: unit.version.as_str().to_owned(),
        source_revision: unit.source_revision.clone(),
        images: unit
            .images
            .iter()
            .map(|(role, image)| {
                (
                    role.clone(),
                    ImageDigest {
                        repository: image.repository.clone(),
                        digest: image.digest.clone(),
                    },
                )
            })
            .collect(),
    }
}
