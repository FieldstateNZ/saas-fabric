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
/// # Only pin-vs-artifact is checked here
///
/// A chart release once had to agree with the artifact *and* the pin,
/// checked together in this function. It no longer does: `write_desired`
/// calls [`identity::check_release`](crate::desired::identity) before a
/// single pin is read, and that call refuses a release whose repository or
/// chart disagrees with what the component publishes — for every component,
/// whether or not it has a pin at all. A call that reaches this function has
/// already passed that check, so the release side of the three-way agreement
/// is no longer this function's to repeat.
///
/// What is left, and still checked here, is the leg `check_release` cannot
/// see: whether *this pin* — the platform repository's own file — names the
/// same chart the manifest's `artifact` says the component is published as.
/// A pin that disagrees with the artifact is the manifest disagreeing with
/// itself, and that can be true independently of what any caller requested.
///
/// `Ok(None)` means this pin has nothing to write for this release — an image
/// the release does not carry — which is not a failure.
///
/// # Errors
///
/// [`Rejected`](PlatformGitError::Rejected) if the pin names an image the
/// component does not publish, if the renderer does not match the artifact,
/// if a chart pin disagrees with the artifact it is pinning, or if the file
/// does not carry the pin the manifest says it does.
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
            WantedVersion::Chart { version, .. },
        ) => {
            // The release already agrees with the artifact — decided by
            // `identity::check_release` before this function was ever
            // called. What is checked here is the pin itself: whether this
            // file names the chart the component is published as, which is a
            // fact about the manifest's own consistency and not about what
            // any caller requested.
            if pinned_chart != published_chart || pinned_repository != published_repository {
                return Err(PlatformGitError::Rejected {
                    detail: format!(
                        "{path}: {component} is published as {published_chart} from \
                         {published_repository}, and this pins {pinned_chart} from \
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
