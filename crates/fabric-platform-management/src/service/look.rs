//! Reading a component, and asking the right registry what exists.

use crate::service::{PlatformError, PlatformManagement};
use crate::{discover, discover_chart, ArtifactSource, Channel, ComponentDesired, Discovery, Version};

/// The release line a search is bounded to, if this version has one.
///
/// The series is the desired version's own line, and it only means something
/// for a prerelease. `0.3.0-preview.9` and `preview.10` are the same line;
/// `0.4.0-preview.1` is a different one, and moving to it is a deliberate act
/// rather than something discovery does on a Tuesday.
///
/// A *stable* version has no such line. Every stable advance changes the core
/// — 7.3.0 to 7.3.1 already does — so applying the rule to it meant a stable
/// component could never advance at all, and would report "nothing newer"
/// however much its repository published. Nothing had noticed because the only
/// managed component was on the preview channel.
///
/// What bounds a stable advance instead is not settled: patch and minor are
/// ordinary, and a major upgrade is not something to take on a sweep. Until
/// that is decided, a stable component should be `manual`.
///
/// # It is one function because it is one rule
///
/// Looking backwards had the same latent defect and for longer: rolling back
/// passed the desired version as the series unconditionally, so a stable
/// component could never be offered `7.3.0` below `7.3.1` — the core differs,
/// exactly as it does going forward. Both directions ask here now, so the two
/// cannot drift apart again.
pub(super) fn series_of(desired: &ComponentDesired) -> Option<&Version> {
    match desired.channel {
        Channel::Preview => Some(&desired.version),
        Channel::Stable => None,
    }
}

impl PlatformManagement {
    /// Reads desired state and asks the registries what exists.
    pub(super) async fn look(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<(ComponentDesired, Discovery), PlatformError> {
        let desired = self.desired_state.component(environment, component).await?;
        let series = series_of(&desired);

        // Two searches, not one generalised one. A chart repository answers a
        // different question with a weaker answer, and squeezing it through
        // the registry port would have meant a digest nobody has and a
        // provenance nobody checked.
        let discovery = match &desired.source {
            ArtifactSource::Oci { repositories } => {
                discover(
                    self.registry.as_ref(),
                    repositories,
                    desired.channel,
                    series,
                    &desired.version,
                )
                .await?
            }
            ArtifactSource::Helm { repository, chart } => {
                discover_chart(
                    self.charts.as_ref(),
                    repository,
                    chart,
                    desired.channel,
                    series,
                    &desired.version,
                )
                .await?
            }
        };

        Ok((desired, discovery))
    }
}
