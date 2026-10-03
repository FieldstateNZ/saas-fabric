//! Which versions a search considers, before any of them is examined.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Channel, Registry, RegistryError, Version};

/// Which side of the desired version a search is looking at.
///
/// The two directions serve two different questions and must not be one
/// parameter with a default. *Above* is what the selector may advance to, and
/// keeping it strictly above is what makes automatic selection unable to move
/// an environment backwards whatever a registry lists. *Below* is what an
/// operator may roll back to, and it exists only because they asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    /// Newer than the desired version.
    Above,

    /// Older than it.
    Below,
}

/// Every version worth considering among `repositories`' tags, newest first.
///
/// A tag that does not parse as a version is not one — the `sha-<commit>`
/// tags a build pushes beside the version, and the `sha256-<hex>` tags the
/// referrers tag schema adds, both drop out here. So does a version in
/// another channel, outside `series` when one is given, or not strictly on
/// the `direction` side of `floor`.
///
/// Which repositories to list is the caller's decision, because it is part of
/// what the kind means: images by role are drawn from **every** role's
/// repository, since a version in two of three is still worth reporting as
/// publishing and asking only one would make the answer depend on which build
/// job pushed first; a described component is drawn from its primary
/// repository alone, because that is where its component descriptor is.
pub(super) async fn candidates(
    registry: &dyn Registry,
    repositories: &[&str],
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
    direction: Direction,
) -> Result<Vec<Version>, RegistryError> {
    let mut seen = BTreeSet::new();

    for repository in repositories {
        for tag in registry.tags(repository).await? {
            let Some(version) = Version::parse(&tag) else {
                continue;
            };

            if version.channel() != channel {
                continue;
            }

            // Strictly, in both directions. The desired version is neither
            // something to advance to nor something to roll back to.
            let wanted = match direction {
                Direction::Above => &version > floor,
                Direction::Below => &version < floor,
            };
            if !wanted {
                continue;
            }

            if series.is_some_and(|series| !version.is_series(series)) {
                continue;
            }

            seen.insert(version);
        }
    }

    let mut candidates: Vec<Version> = seen.into_iter().collect();
    candidates.reverse();

    Ok(candidates)
}

/// The repositories of images by role, in the shape [`candidates`] takes.
pub(super) fn every_role(roles: &BTreeMap<String, String>) -> Vec<&str> {
    roles.values().map(String::as_str).collect()
}
