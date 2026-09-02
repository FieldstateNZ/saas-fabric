//! What an environment could be rolled back to.

use std::collections::BTreeMap;

use crate::discovery::unit::{self, Direction};
use crate::{Channel, Registry, RegistryError, ReleaseUnit, Version};

/// How many versions below the desired one are resolved.
///
/// **This is a latency bound, not a taste one.** Establishing that a version
/// was a whole release means fetching a manifest and a config blob for every
/// image, sequentially — measured at roughly three seconds per version against
/// GHCR — and the whole listing has to fit inside one operator request. Ten
/// candidates was over thirty seconds and would have timed out; five is
/// comfortably inside the budget and further back than an operator rolls.
///
/// Raising it needs concurrency first, not a bigger number.
///
/// The bound is **reported**, not silent — see [`History::more`]. A list that
/// quietly stopped would read as "this is everything there is", which is
/// exactly the sort of convenient near-truth the console is not allowed to
/// tell.
const EXAMINED: usize = 5;

/// The versions an operator may roll back to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct History {
    /// Complete, coherent release units below the desired version, newest
    /// first.
    ///
    /// Whole units, not bare versions: a rollback moves the version *and*
    /// every image digest together, so the candidate an operator picks already
    /// carries what would be written. There is nowhere for a caller to supply
    /// a digest, and nothing for one to disagree with.
    pub units: Vec<ReleaseUnit>,

    /// Whether older versions exist that were not examined.
    pub more: bool,
}

/// Finds what an environment could be rolled back to.
///
/// The same resolution and the same coherence rule as [`discover`](super::discover),
/// in the opposite direction. A version whose images are incomplete or
/// disagree about their source commit is **not offered**: rolling back to a
/// release unit that was never coherent would deploy a composition nobody ever
/// ran.
///
/// Incomplete and incoherent candidates are dropped rather than reported here.
/// Their diagnostics exist to explain why an environment did not *advance*,
/// and an operator looking for somewhere to retreat to has no use for a list
/// of places they cannot go.
///
/// # Errors
///
/// [`RegistryError`] if a registry could not be asked. Nothing is offered from
/// a partial answer: a registry that is briefly down must not make a version
/// look like it has been withdrawn.
pub async fn history(
    registry: &dyn Registry,
    roles: &BTreeMap<String, String>,
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
) -> Result<History, RegistryError> {
    let candidates = unit::candidates(registry, roles, channel, series, floor, Direction::Below).await?;

    let more = candidates.len() > EXAMINED;
    let mut units = Vec::new();

    for version in candidates.into_iter().take(EXAMINED) {
        if let unit::Assembly::Complete(unit) = unit::assemble(registry, roles, &version).await? {
            units.push(unit);
        }
    }

    Ok(History { units, more })
}

/// Resolves one version an operator asked to roll back to.
///
/// # Why this is not "is it in [`history`]?"
///
/// It answers the same question and pays for one version instead of five.
/// Rolling back used to re-derive the whole candidate list to check membership
/// in it, which meant an operator's click paid the listing cost a second time
/// on top of the Git write — and against a real registry that exceeded the
/// request budget and returned `504`.
///
/// The guarantee is unchanged, because membership in that list was never the
/// property that mattered. What matters is that the version is in this
/// component's channel and series, sits strictly below what is desired, and
/// resolves *now* to a complete coherent release unit. That is checked here,
/// on the one version named.
///
/// One consequence, deliberately: a version older than the listing's bound is
/// still rollable if a caller names it. The bound is a limit on what is
/// *offered*, and it would be a strange safety rule that made a real release
/// unrollable because five newer ones existed.
///
/// # Errors
///
/// [`RegistryError`] if a registry could not be asked. `Ok(None)` means the
/// version is not one this component can be rolled back to, which is a
/// different thing from not being able to find out.
pub async fn resolve(
    registry: &dyn Registry,
    roles: &BTreeMap<String, String>,
    channel: Channel,
    series: Option<&Version>,
    floor: &Version,
    wanted: &str,
) -> Result<Option<ReleaseUnit>, RegistryError> {
    let Some(version) = Version::parse(wanted) else {
        return Ok(None);
    };

    // The same three tests `candidates` applies, on one version.
    if version.channel() != channel || &version >= floor {
        return Ok(None);
    }

    if series.is_some_and(|series| !version.is_series(series)) {
        return Ok(None);
    }

    match unit::assemble(registry, roles, &version).await? {
        unit::Assembly::Complete(unit) => Ok(Some(unit)),
        // Incomplete or built twice. Not a release this environment ever ran,
        // whatever a tag listing says.
        unit::Assembly::Incomplete | unit::Assembly::Incoherent => Ok(None),
    }
}
