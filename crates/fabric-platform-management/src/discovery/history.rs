//! What an environment could be rolled back to.

use std::collections::BTreeMap;

use crate::discovery::unit::{self, Direction};
use crate::{Channel, Registry, RegistryError, ReleaseUnit, Version};

/// How many versions below the desired one are resolved.
///
/// A bound, because each candidate costs one registry call per image and an
/// environment may have hundreds of tags behind it. Ten is far more than an
/// operator scrolls and far less than a registry holds.
///
/// The bound is **reported**, not silent — see [`History::more`]. A list that
/// quietly stopped would read as "this is everything there is", which is
/// exactly the sort of convenient near-truth the console is not allowed to
/// tell.
const EXAMINED: usize = 10;

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
