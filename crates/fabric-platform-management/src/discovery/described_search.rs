//! Finding the newest version of a described component an environment may
//! move to (ADR 0026 section 9).

use std::collections::BTreeMap;

use crate::discovery::candidates::{candidates, Direction};
use crate::discovery::{evaluate, DescribedRelease, Discovery, Evaluation, Expectation, InvalidVersion};
use crate::{Channel, Registry, RegistryError, Release, Version};

/// Finds the newest release of a described component an environment may
/// move to.
///
/// Candidates are the **primary** repository's tags alone — that is where
/// the component descriptor is attached, and where a version starts to
/// exist — that parse as a version, belong to `channel`, are in `series`
/// when one is given, and sort strictly after `floor`. Each is evaluated,
/// newest first, by the one rule ([`evaluate`]) with the environment's pins
/// as the expectation, and the first complete one is the answer.
///
/// Every other candidate is still evaluated and reported, for the reason
/// [`discover`](super::discover) gives: a broken version below the one
/// selected is what explains a gap. *Undescribed*, *incoherent* and
/// *invalid* are each their own list, because each is a different thing
/// observed and none may be worded as another. A tag that is gone by the
/// time it is resolved is skipped, not reported: the listing was already
/// out of date, and the next pass lists again.
///
/// A `primary` that is not one of `repositories`' roles has no repository to
/// list, so nothing is found. The platform repository's reader refuses such
/// a component before it gets here.
///
/// # Errors
///
/// [`RegistryError`] if the registry could not be asked. Nothing is decided
/// from a partial answer.
pub async fn discover_described(
    registry: &dyn Registry,
    primary: &str,
    repositories: &BTreeMap<String, String>,
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
) -> Result<Discovery, RegistryError> {
    let mut discovery = Discovery::default();
    let Some(repository) = repositories.get(primary) else {
        return Ok(discovery);
    };
    let expectation = Expectation::Pinned {
        primary,
        repositories,
    };

    let listed = [repository.as_str()];
    for version in candidates(registry, &listed, channel, series, floor, Direction::Above).await? {
        match evaluate(registry, repository, version.as_str(), expectation).await? {
            // Newest first, so the first complete one is the highest.
            Evaluation::Complete(found) => {
                discovery.newer.get_or_insert_with(|| described(*found));
            }
            Evaluation::NotTagged => {}
            Evaluation::Undescribed => discovery.undescribed.push(version),
            Evaluation::Incoherent => discovery.incoherent.push(version),
            Evaluation::Invalid(reason) => discovery.invalid.push(InvalidVersion { version, reason }),
        }
    }

    Ok(discovery)
}

/// A complete evaluation, as the release a write is handed: the unit, the
/// role it was found through, and the component descriptor's digest for the
/// commit message.
pub(super) fn described(found: DescribedRelease) -> Release {
    Release::Described {
        unit: found.unit,
        primary: found.primary,
        descriptor: found.descriptor_digest,
    }
}
