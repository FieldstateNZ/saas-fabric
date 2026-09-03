//! Which renderer writes a pin, and what it is allowed to write.

use crate::components::{repin, retarget, Artifact, Component, Pin};
use crate::desired::WantedVersion;
use crate::PlatformGitError;

/// Rewrites one file the way its pin says to.
///
/// # A renderer that does not match the artifact is a refusal
///
/// The pair is checked, never the pin alone. A Kustomize image pin on a
/// component published as a chart is a manifest disagreeing with itself, and
/// there is no approximate rendering to fall back on.
///
/// `Ok(None)` means this pin has nothing to write for this release — an image
/// the release does not carry — which is not a failure.
///
/// # Errors
///
/// [`Rejected`](PlatformGitError::Rejected) if the pin names an image the
/// component does not publish, if the renderer does not match the artifact, or
/// if the file does not carry the pin the manifest says it does.
pub(super) fn render(
    text: &str,
    path: &str,
    component: &str,
    entry: &Component,
    pin: &Pin,
    wanted: &WantedVersion,
) -> Result<Option<String>, PlatformGitError> {
    match (pin, &entry.artifact, wanted) {
        (
            Pin::KustomizeImage { image: role, .. },
            Artifact::Oci { images, .. },
            WantedVersion::Images(unit),
        ) => {
            // A pin naming an image the component does not publish is the
            // manifest disagreeing with itself, and repinning anyway would
            // write whichever entry happened to match.
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

        (
            Pin::ArgoTargetRevision {
                chart: pinned_chart,
                repository: pinned_repository,
                ..
            },
            Artifact::Helm {
                chart: published_chart,
                repository: published_repository,
            },
            WantedVersion::Chart {
                chart: found_chart,
                repository: found_repository,
                version,
            },
        ) => {
            // Three statements of the same identity: what the component is
            // published as, what discovery found, and what this file pins. A
            // version is only a number, and a number is plausible against the
            // wrong chart — so all three must agree before any of them is
            // written.
            let agreed = published_chart == found_chart
                && published_repository == found_repository
                && pinned_chart == published_chart
                && pinned_repository == published_repository;

            if !agreed {
                return Err(PlatformGitError::Rejected {
                    detail: format!(
                        "{path}: {component} publishes {published_chart} from \
                         {published_repository}, the release is {found_chart} from \
                         {found_repository}, and this pins {pinned_chart} from \
                         {pinned_repository}"
                    ),
                });
            }

            Ok(Some(retarget(text, pinned_repository, pinned_chart, version)?))
        }

        (pin, artifact, _) => Err(PlatformGitError::Rejected {
            detail: format!(
                "{path} renders {} for {component}, which is published as {}",
                pin.describe(),
                artifact.describe()
            ),
        }),
    }
}
