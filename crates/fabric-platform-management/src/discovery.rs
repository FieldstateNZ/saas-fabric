//! Finding the newest version an environment is allowed to move to.

use std::collections::BTreeMap;

mod unit;

#[cfg(test)]
mod discovery_tests;

use crate::{Channel, Registry, RegistryError, Version};

/// One image of a release unit, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImage {
    /// Where it was found.
    pub repository: String,

    /// What to deploy.
    pub digest: String,
}

/// A version of a component, complete and coherent across every image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseUnit {
    /// The version every image carries.
    pub version: Version,

    /// The commit every image agrees it was built from.
    pub source_revision: String,

    /// Images by role.
    pub images: BTreeMap<String, ResolvedImage>,
}

/// What a discovery pass found.
///
/// # Why the rejected versions are reported rather than dropped
///
/// `not_yet` is the case that must not be remembered. A component's images are
/// published by parallel jobs, so a version existing in two repositories and
/// not the third is normally a window of a minute or two — and a discovery
/// that recorded "0.3.0-preview.3 is not a thing" would still believe it an
/// hour later. Every pass recomputes from the registry, and a version listed
/// here is expected to move to `available` on a later one.
///
/// `incoherent` is the opposite: images that all exist and disagree about
/// which commit they came from. That is one version built twice, and no
/// waiting fixes it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Discovery {
    /// The newest complete, coherent version that sorts after the floor.
    pub available: Option<ReleaseUnit>,

    /// Newer versions that are still publishing. Transient — retried, never
    /// remembered.
    pub not_yet: Vec<Version>,

    /// Newer versions whose images disagree about their source commit.
    pub incoherent: Vec<Version>,
}

/// Finds the newest release unit an environment may move to.
///
/// Candidates are every tag in the component's repositories that parses as a
/// version, belongs to `channel`, is in `series` when one is given, and sorts
/// strictly after `floor` — which is what makes automatic selection unable to
/// move an environment backwards, whatever a registry lists.
///
/// They are considered newest first, and the first complete one wins.
///
/// # Errors
///
/// [`RegistryError`] if a registry could not be asked. Nothing is decided from
/// a partial answer: a registry that is down leaves availability stale, which
/// is not the same as a version being gone.
pub async fn discover(
    registry: &dyn Registry,
    roles: &BTreeMap<String, String>,
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
) -> Result<Discovery, RegistryError> {
    let candidates = unit::candidates(registry, roles, channel, series, floor).await?;
    let mut discovery = Discovery::default();

    for version in candidates {
        match unit::assemble(registry, roles, &version).await? {
            unit::Assembly::Complete(release) => {
                discovery.available = Some(release);
                // Newest first, so the first complete one is the answer. The
                // versions above it stay listed as what they are.
                break;
            }
            unit::Assembly::Incomplete => discovery.not_yet.push(version),
            unit::Assembly::Incoherent => discovery.incoherent.push(version),
        }
    }

    Ok(discovery)
}
