//! What a described component could be rolled back to (ADR 0026 section 9).

use std::collections::BTreeMap;

use crate::discovery::candidates::{candidates, Direction};
use crate::discovery::described_search::described;
use crate::discovery::history::EXAMINED;
use crate::discovery::{evaluate, Evaluation, Expectation, History};
use crate::{Channel, Registry, RegistryError, Release, Version};

/// Finds what a described component could be rolled back to.
///
/// The same candidates as [`discover_described`](super::discover_described)
/// in the opposite direction, bounded by the same number of versions
/// examined as [`history`](super::history) — reported through
/// [`History::more`] — and each evaluated by the same rule. Only complete
/// releases are offered: rolling back to a version that was never a release
/// unit would deploy a composition nobody ever ran, and an operator looking
/// for somewhere to retreat to has no use for a list of places they cannot
/// go.
///
/// # Errors
///
/// [`RegistryError`] if the registry could not be asked. Nothing is offered
/// from a partial answer.
pub async fn described_history(
    registry: &dyn Registry,
    primary: &str,
    repositories: &BTreeMap<String, String>,
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
) -> Result<History, RegistryError> {
    let Some(repository) = repositories.get(primary) else {
        return Ok(History::default());
    };
    let expectation = Expectation::Pinned {
        primary,
        repositories,
    };

    let listed = [repository.as_str()];
    let candidates = candidates(registry, &listed, channel, series, floor, Direction::Below).await?;
    let more = candidates.len() > EXAMINED;
    let mut releases = Vec::new();

    for version in candidates.into_iter().take(EXAMINED) {
        match evaluate(registry, repository, version.as_str(), expectation).await? {
            Evaluation::Complete(found) => releases.push(described(*found)),
            Evaluation::NotTagged
            | Evaluation::Undescribed
            | Evaluation::Incoherent
            | Evaluation::Invalid(_) => {}
        }
    }

    Ok(History { releases, more })
}

/// Resolves one version of a described component an operator asked to roll
/// back to.
///
/// The picker is navigation and this is validation, exactly as
/// [`resolve`](super::resolve) says for images by role: the version must be
/// in the channel and series, strictly below what is desired, and — *now*,
/// on this request — a complete release unit by the one rule. A version
/// older than the listing's bound is still rollable if a caller names it.
///
/// # Errors
///
/// [`RegistryError`] if the registry could not be asked. `Ok(None)` means
/// the version is not one this component can be rolled back to, which is a
/// different thing from not being able to find out.
pub async fn resolve_described(
    registry: &dyn Registry,
    primary: &str,
    repositories: &BTreeMap<String, String>,
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
    wanted: &str,
) -> Result<Option<Release>, RegistryError> {
    let Some(repository) = repositories.get(primary) else {
        return Ok(None);
    };
    let Some(version) = Version::parse(wanted) else {
        return Ok(None);
    };

    // The same three tests the listing applies, on one version.
    if version.channel() != channel || &version >= floor {
        return Ok(None);
    }
    if series.is_some_and(|series| !version.is_series(series)) {
        return Ok(None);
    }

    let expectation = Expectation::Pinned {
        primary,
        repositories,
    };
    match evaluate(registry, repository, version.as_str(), expectation).await? {
        Evaluation::Complete(found) => Ok(Some(described(*found))),
        // Not a release this environment could ever have run, whatever a
        // tag listing says.
        Evaluation::NotTagged | Evaluation::Undescribed | Evaluation::Incoherent | Evaluation::Invalid(_) => {
            Ok(None)
        }
    }
}
