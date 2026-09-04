//! Whether one named chart version is somewhere this component can go.

use crate::discovery::charts::chart_release;
use crate::{Channel, ChartIndex, RegistryError, Release, Version};

/// Resolves one chart version an operator asked to roll back to.
///
/// The mirror of [`resolve`](super::resolve), and for the same reason: it
/// answers whether this one version is somewhere this component can go, and
/// pays for one version rather than a whole listing. Membership in the offered
/// list was never the property that mattered, so a version older than the
/// listing's bound is still rollable if a caller names it.
///
/// Parsed with [`parse_chart`](Version::parse_chart) rather than
/// [`parse`](Version::parse): Helm permits build metadata and Argo pins
/// whatever the index carries, so reading `7.3.1+build.7` with the OCI grammar
/// would refuse a version this platform had itself written.
///
/// # Errors
///
/// [`RegistryError`] if the chart repository could not be asked. `Ok(None)`
/// means the version is not one this component can be rolled back to, which is
/// a different thing from not being able to find out.
pub async fn resolve_chart(
    charts: &dyn ChartIndex,
    repository: &str,
    chart: &str,
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
    wanted: &str,
) -> Result<Option<Release>, RegistryError> {
    let Some(version) = Version::parse_chart(wanted) else {
        return Ok(None);
    };

    // The same three tests the listing applies, on one version.
    if version.channel() != channel || &version >= floor {
        return Ok(None);
    }

    if series.is_some_and(|series| !version.is_series(series)) {
        return Ok(None);
    }

    // Asked of the index rather than assumed: a version withdrawn since it ran
    // is not somewhere to return to, and this is the check that notices.
    //
    // What comes back is the index's version and not the caller's, because
    // build metadata takes no part in precedence -- `7.3.0` and `7.3.0+evil`
    // compare equal, so a caller could otherwise have a spelling the
    // repository never published written verbatim into the pin.
    let listed = charts
        .versions(repository, chart)
        .await?
        .into_iter()
        .find(|published| published == &version);

    Ok(listed.map(|version| chart_release(repository, chart, version)))
}
