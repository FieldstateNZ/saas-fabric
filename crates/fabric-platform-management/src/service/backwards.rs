//! Asking the right registry what is *below* the version in use.
//!
//! The mirror of [`look`](super::look), and deliberately shaped like it: the
//! kind decides which port answers, and both directions read the series rule
//! from the same place. Rollback used to dispatch nowhere at all — it took the
//! OCI repositories or refused — which is why this file did not exist.

use crate::service::look::series_of;
use crate::{
    chart_history, history, resolve, resolve_chart, ArtifactSource, ChartIndex, ComponentDesired, History,
    Registry, RegistryError, Release,
};

/// What this component could be rolled back to, whichever kind it is.
///
/// # Errors
///
/// [`RegistryError`] if the registry or the chart repository could not be
/// asked. Nothing is offered from a partial answer: something briefly down
/// must not make a version look as though it has been withdrawn.
pub(super) async fn candidates(
    registry: &dyn Registry,
    charts: &dyn ChartIndex,
    desired: &ComponentDesired,
) -> Result<History, RegistryError> {
    // Preview only, for the same reason a forward search bounds itself that
    // way -- stated once, on `series_of` in `look.rs`. Passing the desired
    // version unconditionally is the defect this replaces: a stable component
    // could never be offered 7.3.0 below 7.3.1, because their cores differ.
    let series = series_of(desired);

    match &desired.source {
        ArtifactSource::Oci { repositories } => {
            history(registry, repositories, desired.channel, series, &desired.version).await
        }
        ArtifactSource::Helm { repository, chart } => {
            chart_history(
                charts,
                repository,
                chart,
                desired.channel,
                series,
                &desired.version,
            )
            .await
        }
    }
}

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
pub(super) async fn one(
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
    }
}
