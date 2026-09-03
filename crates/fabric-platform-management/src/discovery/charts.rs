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
        newer: newest.map(|version| Release::Chart { version }),
        not_yet: Vec::new(),
        incoherent: Vec::new(),
    })
}
