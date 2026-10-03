//! Finding the newest version an environment is allowed to move to.

use std::collections::BTreeMap;

mod candidates;
mod chart_history;
mod chart_resolve;
mod charts;
mod described;
mod described_history;
mod described_search;
mod found;
mod history;
mod unit;

pub use chart_history::chart_history;
pub use chart_resolve::resolve_chart;
pub use charts::discover_chart;
pub use described::{evaluate, DescribedRelease, Evaluation, Expectation, InvalidReason, RevisionOf};
pub use described_history::{described_history, resolve_described};
pub use described_search::discover_described;
pub use found::{Discovery, InvalidVersion};
pub use history::{history, resolve, History};

#[cfg(test)]
mod described_search_tests;
#[cfg(test)]
pub(crate) use described::fake_registry_tests as described_fake_registry;
#[cfg(test)]
mod discovery_tests;

use crate::{Channel, Registry, RegistryError, Version};
use candidates::{candidates, every_role, Direction};

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
    let repositories = every_role(roles);
    let candidates = candidates(registry, &repositories, channel, series, floor, Direction::Above).await?;
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
