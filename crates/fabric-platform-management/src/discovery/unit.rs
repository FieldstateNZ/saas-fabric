//! Assembling a version's images into a release unit, or declining to.

use std::collections::{BTreeMap, BTreeSet};

use crate::discovery::{ReleaseUnit, ResolvedImage};
use crate::{Channel, Provenance, Registry, RegistryError, Version};

/// What a version turned out to be.
pub(super) enum Assembly {
    /// Every image exists and agrees where it came from.
    Complete(ReleaseUnit),

    /// At least one image is not published yet. Transient.
    Incomplete,

    /// Every image exists, and they do not agree on a source commit.
    Incoherent,
}

/// Every version worth considering, newest first.
///
/// Drawn from **every** repository's tags rather than one nominated as the
/// leader. A version that appears in two of three is still a version worth
/// looking at — reporting it as publishing is the whole point — and asking
/// only the first repository would make the answer depend on which image its
/// build job happened to push first.
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

pub(super) async fn candidates(
    registry: &dyn Registry,
    roles: &BTreeMap<String, String>,
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
    direction: Direction,
) -> Result<Vec<Version>, RegistryError> {
    let mut seen = BTreeSet::new();

    for repository in roles.values() {
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

/// Resolves one version across every repository.
pub(super) async fn assemble(
    registry: &dyn Registry,
    roles: &BTreeMap<String, String>,
    version: &Version,
) -> Result<Assembly, RegistryError> {
    let mut images = BTreeMap::new();
    let mut revisions = BTreeSet::new();

    for (role, repository) in roles {
        let Some(resolved) = registry.resolve(repository, version.as_str()).await? else {
            // Not published *yet*. Nothing is recorded about this version, so
            // the next pass asks again from nothing.
            return Ok(Assembly::Incomplete);
        };

        match resolved.provenance {
            // Indistinguishable from a push still in flight, and waiting is
            // the cheaper mistake.
            Provenance::Absent => return Ok(Assembly::Incomplete),

            // The artifact's own parts disagree about where they came from.
            // That is one version built twice, one level below the check
            // across repositories below, and no waiting fixes it.
            Provenance::Disagreed => return Ok(Assembly::Incoherent),

            Provenance::Agreed(revision) => revisions.insert(revision),
        };
        images.insert(
            role.clone(),
            ResolvedImage {
                repository: repository.clone(),
                digest: resolved.digest,
            },
        );
    }

    if images.is_empty() {
        return Ok(Assembly::Incomplete);
    }

    // One commit, or it is not one release. Images built from different
    // commits under one version is the case that would otherwise put a console
    // from one build beside a control plane from another.
    let mut revisions = revisions.into_iter();
    let (Some(revision), None) = (revisions.next(), revisions.next()) else {
        return Ok(Assembly::Incoherent);
    };

    Ok(Assembly::Complete(ReleaseUnit {
        version: version.clone(),
        source_revision: revision,
        images,
    }))
}
