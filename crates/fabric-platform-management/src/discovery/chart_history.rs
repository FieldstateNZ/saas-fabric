//! What a chart repository holds below the version in use.

use crate::discovery::charts::chart_release;
use crate::discovery::history::EXAMINED;
use crate::{Channel, ChartIndex, History, RegistryError, Version};

/// Finds what a chart component could be rolled back to.
///
/// The mirror of [`history`](super::history) for the other artifact kind, and
/// the same three tests in the same order: in the channel, in the series when
/// one is given, and strictly below the desired version. Newest first, bounded
/// by the same five and reporting `more` the same way. Reading an index costs
/// almost nothing, so that bound is not the latency budget it is on the image
/// path — it is so the console meets one shape whichever kind of component an
/// operator is looking at.
///
/// # Nothing to assemble, and nothing to reject
///
/// The image path drops candidates whose images are missing or disagree about
/// their commit, because a version that was never one coherent release is
/// nowhere an environment can return to. A chart is one artifact: if the index
/// lists a version, this component was published as it. So every candidate
/// inside the bound is offered.
///
/// # What a candidate promises, and what it does not
///
/// The version, and not the bytes: a chart repository pins a version rather
/// than a digest, so what sits behind `7.3.0` may have been republished since
/// this environment ran it. That is said to the operator in the console rather
/// than being a reason to offer nothing — refusing would leave someone whose
/// chart upgrade went wrong with no route back but a hand edit.
///
/// # Errors
///
/// [`RegistryError`] if the chart repository could not be asked. Nothing is
/// offered from a partial answer: a repository that is briefly down must not
/// make a version look like it has been withdrawn.
pub async fn chart_history(
    charts: &dyn ChartIndex,
    repository: &str,
    chart: &str,
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
) -> Result<History, RegistryError> {
    let mut candidates: Vec<Version> = charts
        .versions(repository, chart)
        .await?
        .into_iter()
        .filter(|version| version.channel() == channel && version < floor)
        .filter(|version| series.is_none_or(|series| version.is_series(series)))
        .collect();

    // Newest first, which is the order an operator reads a rollback list in.
    candidates.sort_unstable();
    candidates.reverse();

    let more = candidates.len() > EXAMINED;

    Ok(History {
        releases: candidates
            .into_iter()
            .take(EXAMINED)
            .map(|version| chart_release(repository, chart, version))
            .collect(),
        more,
    })
}
