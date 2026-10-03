//! Recording a new version in the manifest entry itself.

use crate::components::{Artifact, Component};
use crate::desired::{identity, WantedVersion};
use crate::PlatformGitError;

/// Records the new version in the manifest entry.
///
/// Only the version, the source commit and each image's digest. The channel,
/// the update policy, the hold, every `pinnedIn` and a described component's
/// primary are the platform repository's, and survive untouched.
///
/// # An exhaustive match, not a `let … else`
///
/// This used to destructure the one images pair and return early for
/// anything else, having already written the version. A third kind then
/// compiled and wrote only `desired.version` — no commit, no digests — which
/// is the one way to get this wrong that nothing notices. Every pair is now
/// named, both image kinds write the same three things, and a pair
/// `check_release` would have refused is refused again here, before
/// anything is written, rather than written approximately.
///
/// # Errors
///
/// [`Rejected`](PlatformGitError::Rejected) for a release shaped for another
/// kind than the component's.
pub(super) fn apply(
    component: &str,
    entry: &mut Component,
    wanted: &WantedVersion,
) -> Result<(), PlatformGitError> {
    let agrees = match (&mut entry.artifact, wanted) {
        (
            Artifact::Oci {
                source_revision,
                images,
            },
            WantedVersion::Images(unit),
        )
        | (
            Artifact::Described {
                source_revision,
                images,
                ..
            },
            WantedVersion::Described { version: unit, .. },
        ) => {
            source_revision.clone_from(&unit.source_revision);
            for (role, image) in images {
                if let Some(offered) = unit.images.get(role) {
                    image.digest.clone_from(&offered.digest);
                }
            }
            true
        }

        // A chart's version is the whole of its desired state. There is no
        // provenance to record and no digest to move.
        (Artifact::Helm { .. }, WantedVersion::Chart { .. }) => true,

        (Artifact::Oci { .. }, WantedVersion::Chart { .. } | WantedVersion::Described { .. })
        | (Artifact::Helm { .. }, WantedVersion::Images(_) | WantedVersion::Described { .. })
        | (Artifact::Described { .. }, WantedVersion::Images(_) | WantedVersion::Chart { .. }) => false,
    };

    if !agrees {
        return Err(identity::mismatch(component, entry, wanted));
    }

    wanted.version().clone_into(&mut entry.desired.version);
    Ok(())
}
