//! Refusing a request that is not this component's release.
//!
//! One closely related set of pure functions: every rule that refuses a
//! release before a file is read, kept together because none of them is
//! useful, or even meaningful, apart from the shape they all check —
//! whether this request is a version of *this* component.

mod rules;

use crate::components::{Artifact, Component};
use crate::desired::WantedVersion;
use crate::PlatformGitError;

use rules::{check_chart, check_images, check_primary};

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
/// # Every combination, each decided explicitly
///
/// - **Images against an OCI artifact** — the roles must match exactly, and
///   no image may move to a different registry.
/// - **A chart against a Helm artifact** — the release's repository and
///   chart must equal the artifact's, byte for byte. A version is only a
///   number, and a number is plausible against the wrong chart, so the
///   identity around it has to agree too.
/// - **A described release against a described artifact** — the images
///   exactly as for OCI, and the primary it was found through must be the
///   one the manifest names.
/// - **Any other pair** — refused outright. A release shaped for another
///   kind is not a version of this component, whatever number it carries;
///   there is no partial agreement left to check. That includes images
///   against a described artifact and the reverse: moving a component
///   between being found by role and being found through its component
///   descriptor is an edit of the platform repository, never a version.
///
/// No arm falls through to a default `Ok(())`, and none of them can: the
/// mismatched combinations are written out explicitly rather than caught by
/// a wildcard, so another `Artifact` variant leaves this match
/// non-exhaustive and refuses to compile, instead of quietly refusing every
/// release at runtime. That wildcard is the defect this replaces — it let a
/// mismatched shape reach `apply`, which writes whatever version string it
/// is given into `desired.version` without knowing it disagreed with the
/// artifact.
///
/// # Errors
///
/// [`Rejected`](PlatformGitError::Rejected) if the release's shape does not
/// match the artifact, if a chart's repository or name does not match
/// exactly, or if a described release was found through another primary.
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

        (
            Artifact::Described { primary, images, .. },
            WantedVersion::Described {
                version,
                primary: found,
            },
        ) => {
            check_images(component, images, version)?;
            check_primary(component, primary, found)
        }

        (Artifact::Oci { .. }, WantedVersion::Chart { .. } | WantedVersion::Described { .. })
        | (Artifact::Helm { .. }, WantedVersion::Images(_) | WantedVersion::Described { .. })
        | (Artifact::Described { .. }, WantedVersion::Images(_) | WantedVersion::Chart { .. }) => {
            Err(mismatch(component, entry, wanted))
        }
    }
}

/// A release shaped for another kind than the component's.
pub(super) fn mismatch(component: &str, entry: &Component, wanted: &WantedVersion) -> PlatformGitError {
    PlatformGitError::Rejected {
        detail: format!(
            "{component} publishes {}, and the request carries {}",
            entry.artifact.describe(),
            wanted.describe(),
        ),
    }
}
