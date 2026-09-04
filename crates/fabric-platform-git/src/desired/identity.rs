//! Refusing a request that is not this component's release.
//!
//! One closely related set of pure functions: every rule that refuses a
//! release before a file is read, kept together because none of them is
//! useful, or even meaningful, apart from the shape they all check —
//! whether this request is a version of *this* component.

use std::collections::BTreeMap;

use crate::components::{Artifact, Component, ImagePin};
use crate::desired::{ComponentVersion, WantedVersion};
use crate::PlatformGitError;

/// Refuses a request whose release does not match what the component
/// publishes, before any pin is read or any file is rendered.
///
/// # Why this runs whether or not `pinnedIn` is empty
///
/// A component with no pins still has an artifact kind, and for a chart a
/// repository and a name — and a request that disagrees with either is wrong
/// regardless of whether anything downstream would have gone looking for a
/// file to write. Deciding shape here, before [`rewrite_pins`](super::plan::rewrite_pins)
/// touches a single file, is what makes that true for every component,
/// rather than only the ones somebody remembered to pin.
///
/// # Four combinations, each decided explicitly
///
/// - **Images against an OCI artifact** — the existing rule: the roles must
///   match exactly, and no image may move to a different registry.
/// - **A chart against a Helm artifact** — the release's repository and
///   chart must equal the artifact's, byte for byte. A version is only a
///   number, and a number is plausible against the wrong chart, so the
///   identity around it has to agree too.
/// - **A chart against an OCI artifact, or images against a Helm artifact**
///   — refused outright. A release shaped for the other kind is not a
///   version of this component, whatever number it carries; there is no
///   partial agreement left to check.
///
/// No arm falls through to a default `Ok(())`, and none of them can: the two
/// mismatched combinations are written out explicitly rather than caught by
/// a wildcard, so a third `Artifact` variant would leave this match
/// non-exhaustive and refuse to compile, instead of quietly refusing every
/// release at runtime. That wildcard is the defect this replaces — it let a
/// mismatched shape reach `apply`, which writes whatever version string it
/// is given into `desired.version` without knowing it disagreed with the
/// artifact.
///
/// # Errors
///
/// [`Rejected`](PlatformGitError::Rejected) if the release's shape does not
/// match the artifact, or if a chart's repository or name does not match
/// exactly.
pub(super) fn check_release(
    component: &str,
    entry: &Component,
    wanted: &WantedVersion,
) -> Result<(), PlatformGitError> {
    match (&entry.artifact, wanted) {
        (Artifact::Oci { images, .. }, WantedVersion::Images(unit)) => check_images(component, images, unit),

        (
            Artifact::Helm { repository, chart },
            WantedVersion::Chart {
                repository: found_repository,
                chart: found_chart,
                ..
            },
        ) => check_chart(component, repository, chart, found_repository, found_chart),

        (Artifact::Oci { .. }, WantedVersion::Chart { .. })
        | (Artifact::Helm { .. }, WantedVersion::Images(_)) => Err(PlatformGitError::Rejected {
            detail: format!(
                "{component} publishes {}, and the request carries {}",
                entry.artifact.describe(),
                wanted.describe(),
            ),
        }),
    }
}

/// The OCI rule: roles agree exactly, and no role's repository moves.
///
/// Two rules, and they are the same rule from two sides: a caller may move a
/// component to a new *version*, and to nothing else.
fn check_images(
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
fn check_chart(
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
