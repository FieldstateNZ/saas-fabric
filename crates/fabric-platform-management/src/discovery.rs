//! Finding the newest version an environment is allowed to move to.

use std::collections::BTreeMap;

mod chart_history;
mod chart_resolve;
mod charts;
mod history;
mod unit;

pub use chart_history::chart_history;
pub use chart_resolve::resolve_chart;
pub use charts::discover_chart;
pub use history::{history, resolve, History};

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
/// here is expected to move to `newer` on a later one.
///
/// `incoherent` is the opposite: images that all exist and disagree about
/// which commit they came from. That is one version built twice, and no
/// waiting fixes it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Discovery {
    /// The newest complete, coherent version that sorts after the floor.
    ///
    /// # Not "the available version"
    ///
    /// It is the newest eligible version *newer than desired*, which is a
    /// narrower fact and needs the narrower name. Nothing here observes
    /// whether the desired version itself is still in the registry, so a
    /// broader name would be a claim this type is not entitled to make — and
    /// the console said exactly that for a while, rendering `Available —`
    /// about an environment running the newest preview there was.
    ///
    /// A `Latest available` worth the name arrives with a versions view, where
    /// Fabric enumerates what exists rather than inferring it from what it
    /// declined to advance to.
    pub newer: Option<crate::Release>,

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
/// They are considered newest first, and the first complete one is the answer.
///
/// Every other candidate is still examined, and that is deliberate. A broken
/// version *below* the one selected is what explains a gap: an environment
/// moving from `preview.2` to `preview.4` should be able to say what happened
/// to `preview.3` rather than silently skipping it. The cost is proportional
/// to how far behind the environment is, which is the right shape — an
/// environment advancing normally examines one or two, and one that has been
/// held for a month examines a month's worth, which is exactly when the
/// diagnostics are worth having.
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
    let candidates =
        unit::candidates(registry, roles, channel, series, floor, unit::Direction::Above).await?;
    let mut discovery = Discovery::default();

    for version in candidates {
        match unit::assemble(registry, roles, &version).await? {
            // Newest first, so the first complete one is the highest.
            unit::Assembly::Complete(release) => discovery.newer.get_or_insert(crate::Release::Unit(release)),
            unit::Assembly::Incomplete => {
                discovery.not_yet.push(version);
                continue;
            }
            unit::Assembly::Incoherent => {
                discovery.incoherent.push(version);
                continue;
            }
        };
    }

    Ok(discovery)
}
