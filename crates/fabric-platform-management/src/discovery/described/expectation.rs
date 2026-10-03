//! The last check of step 3: whether the component descriptor is what the
//! caller already knows the component must be.

use fabric_component::{ComponentDescriptor, Role};

use super::{Expectation, InvalidReason};

/// Why the component descriptor is not what `expectation` says, or `None`
/// when it is.
///
/// # Pinned: equal, not contained
///
/// The environment's pins and the component descriptor must name the same
/// roles, each at the same repository, and the same primary. A role the
/// descriptor adds is an image this environment would never deploy, and a
/// role it drops is one the write would refuse to move without; either way
/// the release is not one these pins can take, and a write would refuse it
/// (`fabric-platform-git`'s identity check) — so discovery says so, where it
/// was observed, rather than offering a version that could never be written.
///
/// # Registered: the first, in role order
///
/// Every repository the component descriptor names must be registered. The
/// first that is not is named, and role order makes that the same one on
/// every read.
pub(super) fn check(
    descriptor: &ComponentDescriptor,
    primary: &Role,
    expectation: Expectation<'_>,
) -> Option<InvalidReason> {
    let images = &descriptor.spec().images;

    match expectation {
        Expectation::Pinned {
            primary: pinned,
            repositories,
        } => {
            // Both maps iterate in ascending role order, so equal lengths and
            // pairwise equality is equality of the two maps.
            let same = images.len() == repositories.len()
                && images.iter().zip(repositories).all(
                    |((role, image), (pinned_role, pinned_repository))| {
                        role.as_str() == pinned_role && image.repository.as_str() == pinned_repository
                    },
                );

            (!same || primary.as_str() != pinned).then_some(InvalidReason::NotPinned)
        }

        Expectation::Registered(registered) => images
            .values()
            .find(|image| !registered(image.repository.as_str()))
            .map(|image| InvalidReason::NotRegistered {
                repository: image.repository.as_str().to_owned(),
            }),
    }
}
