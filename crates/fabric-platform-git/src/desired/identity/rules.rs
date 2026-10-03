//! The rules a release of the right shape must still pass: the same roles
//! at the same repositories, the same chart, the same primary.

use std::collections::BTreeMap;

use crate::components::ImagePin;
use crate::desired::ComponentVersion;
use crate::PlatformGitError;

/// The OCI rule: roles agree exactly, and no role's repository moves.
///
/// Two rules, and they are the same rule from two sides: a caller may move a
/// component to a new *version*, and to nothing else.
pub(super) fn check_images(
    component: &str,
    images: &BTreeMap<String, ImagePin>,
    wanted: &ComponentVersion,
) -> Result<(), PlatformGitError> {
    let declared: Vec<&String> = images.keys().collect();
    let offered: Vec<&String> = wanted.images.keys().collect();

    if declared != offered {
        return Err(PlatformGitError::Rejected {
            detail: format!(
                "{component} publishes {declared:?} and they move together; the request carries {offered:?}"
            ),
        });
    }

    // Both are `BTreeMap`s, so both iterate in ascending key order — and the
    // check above just proved the two key sets are equal. `zip` therefore
    // pairs each role with itself, not with whatever the next key happens to
    // be, so there is no missing-role case left for this loop to handle.
    for ((role, image), (_, offered)) in images.iter().zip(&wanted.images) {
        // A version change may not become a registry change. Where a component
        // is published is the platform repository's statement, not a caller's.
        if offered.repository != image.repository {
            return Err(PlatformGitError::Rejected {
                detail: format!(
                    "{component}/{role} is published to '{}', and the request names '{}'",
                    image.repository, offered.repository
                ),
            });
        }
    }

    Ok(())
}

/// The Helm rule: a chart's repository and name must match exactly.
///
/// Byte-equal, with no trimming or normalisation — the identity a release is
/// checked against is what the platform repository wrote, not a
/// caller-friendly approximation of it.
pub(super) fn check_chart(
    component: &str,
    repository: &str,
    chart: &str,
    found_repository: &str,
    found_chart: &str,
) -> Result<(), PlatformGitError> {
    if repository == found_repository && chart == found_chart {
        return Ok(());
    }

    Err(PlatformGitError::Rejected {
        detail: format!(
            "{component} is published as '{chart}' from '{repository}', and the request names \
             '{found_chart}' from '{found_repository}'"
        ),
    })
}

/// The described rule's addition: the release was found through the image
/// the manifest names as primary.
///
/// A component descriptor attached to another role's image is a different
/// statement about the component — its versions would be listed from
/// another repository — so a release found that way is not a version of
/// this component, however well its images match.
pub(super) fn check_primary(component: &str, primary: &str, found: &str) -> Result<(), PlatformGitError> {
    if primary == found {
        return Ok(());
    }

    Err(PlatformGitError::Rejected {
        detail: format!(
            "{component} is described through its '{primary}' image, and the request was found through '{found}'"
        ),
    })
}
