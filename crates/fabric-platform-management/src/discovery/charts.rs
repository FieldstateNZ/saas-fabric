//! What a chart repository offers above the version in use.

use crate::discovery::Discovery;
use crate::{Channel, ChartIndex, RegistryError, Release, Version};

/// Finds the newest chart version an environment may move to.
///
/// The same rule as the image path — newest in the channel and series,
/// strictly above the floor — and none of the machinery underneath it. There
/// is nothing to assemble from several repositories, nothing to agree about a
/// source commit, and no digest to pin.
///
/// The identity travels with the version, so the write can refuse a release
/// discovered somewhere other than where the pin says to put it.
///
/// # `not_yet` and `incoherent` stay empty, and that is not a gap
///
/// Those diagnostics exist because a component published as three images can
/// be half-published, or built twice. A chart is one artifact: it cannot be
/// partly there and it cannot disagree with itself. There is nothing to
/// report, rather than something going unreported.
///
/// # Errors
///
/// [`RegistryError`] if the chart repository could not be asked.
pub async fn discover_chart(
    charts: &dyn ChartIndex,
    repository: &str,
    chart: &str,
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
) -> Result<Discovery, RegistryError> {
    let newest = charts
        .versions(repository, chart)
        .await?
        .into_iter()
        .filter(|version| version.channel() == channel && version > floor)
        .filter(|version| series.is_none_or(|series| version.is_series(series)))
        .max();

    Ok(Discovery {
        newer: newest.map(|version| chart_release(repository, chart, version)),
        not_yet: Vec::new(),
        incoherent: Vec::new(),
    })
}

/// A chart version, carrying the identity it was discovered under.
///
/// A bare number says nothing about *what* it is a version of, and is
/// plausible against the wrong chart: discovery found `7.3.1` of one chart in
/// one repository, a pin names a chart in a repository too, and if the write
/// does not compare them then a release discovered from one chart can be
/// written into a pin for another.
///
/// One constructor rather than three literals, because all three chart
/// searches — what is newer, what is below, and whether one named version is —
/// have to attach the same identity, and one that quietly did not would be a
/// number the write had nothing to check.
pub(super) fn chart_release(repository: &str, chart: &str, version: Version) -> Release {
    Release::Chart {
        repository: repository.to_owned(),
        chart: chart.to_owned(),
        version,
    }
}
