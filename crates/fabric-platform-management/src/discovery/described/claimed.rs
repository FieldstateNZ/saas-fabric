//! What steps 1 to 3 of the rule established, and the release unit it
//! becomes once steps 4 and 5 pass.

use std::collections::BTreeMap;

use fabric_component::ComponentDescriptor;

use super::DescribedRelease;
use crate::discovery::{ReleaseUnit, ResolvedImage};
use crate::{AttachedDescriptor, Resolved, Version};

/// What steps 1 to 3 established, handed to steps 4 and 5.
pub(super) struct Claimed {
    /// The version, parsed, equal to the tag and to what the descriptor says.
    pub(super) version: Version,

    /// The role the descriptor names the image it is attached to by.
    pub(super) primary: String,

    /// What the primary repository's version tag resolved to.
    pub(super) primary_image: Resolved,

    /// The component descriptor, valid.
    pub(super) descriptor: ComponentDescriptor,

    /// The component descriptor as it was attached: its digest and its
    /// annotations.
    pub(super) attached: AttachedDescriptor,
}

/// The release unit, from the component descriptor's images and the digests
/// Fabric computed for them.
pub(super) fn complete(
    claimed: Claimed,
    found: BTreeMap<String, Resolved>,
    source_revision: String,
) -> DescribedRelease {
    let mut digests: BTreeMap<String, String> = found
        .into_iter()
        .map(|(role, resolved)| (role, resolved.digest))
        .collect();
    digests.insert(claimed.primary.clone(), claimed.primary_image.digest);

    let images = claimed
        .descriptor
        .spec()
        .images
        .iter()
        .filter_map(|(role, image)| {
            let digest = digests.remove(role.as_str())?;
            let repository = image.repository.as_str().to_owned();
            Some((role.as_str().to_owned(), ResolvedImage { repository, digest }))
        })
        .collect();

    DescribedRelease {
        unit: ReleaseUnit {
            version: claimed.version,
            source_revision,
            images,
        },
        primary: claimed.primary,
        descriptor_digest: claimed.attached.digest,
        descriptor: claimed.descriptor,
    }
}
