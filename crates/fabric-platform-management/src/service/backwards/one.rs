//! Resolving the one version an operator named, for every kind.

use crate::service::look::series_of;
use crate::{
    resolve, resolve_chart, resolve_described, ArtifactSource, ChartIndex, ComponentDesired, Registry,
    RegistryError, Release,
};

/// Resolves the one version an operator named, whichever kind it is.
///
/// One version, not the listing again. Re-deriving the whole listing to check
/// membership made an operator's click pay for five versions plus a Git write,
/// which exceeded the request budget against a real registry and answered 504.
///
/// # Errors
///
/// [`RegistryError`] if the registry or the chart repository could not be
/// asked. `Ok(None)` means the version is not one this component can be rolled
/// back to, which is a different thing from not being able to find out.
pub(in crate::service) async fn one(
    registry: &dyn Registry,
    charts: &dyn ChartIndex,
    desired: &ComponentDesired,
    wanted: &str,
) -> Result<Option<Release>, RegistryError> {
    // The same rule as the listing above, so the two cannot disagree about
    // what is eligible.
    let series = series_of(desired);

    match &desired.source {
        ArtifactSource::Oci { repositories } => {
            resolve(
                registry,
                repositories,
                desired.channel,
                series,
                &desired.version,
                wanted,
            )
            .await
        }
        ArtifactSource::Helm { repository, chart } => {
            resolve_chart(
                charts,
                repository,
                chart,
                desired.channel,
                series,
                &desired.version,
                wanted,
            )
            .await
        }
        ArtifactSource::Described {
            primary,
            repositories,
        } => {
            resolve_described(
                registry,
                primary,
                repositories,
                desired.channel,
                series,
                &desired.version,
                wanted,
            )
            .await
        }
    }
}
