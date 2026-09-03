//! Turning a release into what a write speaks.

use fabric_platform_management::{Release, ReleaseUnit};

use crate::desired::{ComponentVersion, ImageDigest, WantedVersion};

/// What a write is asked to make true.
///
/// Whole-release in, whole-release out. For images that means the version, the
/// source commit and every digest travelling together, so there is no shape in
/// which a caller could move one image or supply a digest of their own. For a
/// chart it means the version, which is the whole of what Argo pins.
pub(super) fn wanted_from(release: &Release) -> WantedVersion {
    match release {
        Release::Unit(unit) => WantedVersion::Images(unit_from(unit)),
        Release::Chart { version } => WantedVersion::Chart {
            version: version.as_str().to_owned(),
        },
    }
}

/// A release unit, in the vocabulary the write speaks.
pub(super) fn unit_from(unit: &ReleaseUnit) -> ComponentVersion {
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
