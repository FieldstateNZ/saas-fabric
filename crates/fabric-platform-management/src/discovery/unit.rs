//! Assembling a version's images into a release unit, or declining to.

use std::collections::{BTreeMap, BTreeSet};

use crate::discovery::{ReleaseUnit, ResolvedImage};
use crate::{Provenance, Registry, RegistryError, Version};

/// What a version turned out to be.
pub(super) enum Assembly {
    /// Every image exists and agrees where it came from.
    Complete(ReleaseUnit),

    /// At least one image is not published yet. Transient.
    Incomplete,

    /// Every image exists, and they do not agree on a source commit.
    Incoherent,
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
