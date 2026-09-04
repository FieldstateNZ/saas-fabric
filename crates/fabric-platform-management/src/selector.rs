//! Whether to move an environment, given what it runs and what exists.

#[cfg(test)]
mod selector_tests;

use crate::{Channel, Discovery, Release, UpdatePolicy};

/// Why nothing is being done.
///
/// Carried rather than discarded so the console can say *why* an environment
/// is not advancing. "Nothing is happening" and "nothing is happening because
/// an operator paused it" look identical from the outside and are not the same
/// situation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The policy asks an operator to choose.
    Manual,

    /// The policy is pinned.
    Locked,
    /// Advancement is paused by an operator's hold.
    Held,

    /// There is nothing newer to move to.
    NothingNewer,

    /// Nobody has said what an automatic *stable* advance is allowed to do.
    ///
    /// # Failing closed, deliberately
    ///
    /// A prerelease advances within its line, and the line is what stops it
    /// crossing to another. A stable version has no such line: every stable
    /// advance changes the core, so the rule that bounds a preview bounds
    /// nothing here — and without one, `7.3.0` to `8.0.0` is a decision a
    /// sweep would take on its own at three in the morning.
    ///
    /// Patch and minor upgrades are ordinary and a major is not, and which of
    /// them an automatic policy may take has not been decided. Until it is,
    /// this combination advances nothing and says why. A stable component that
    /// should move is `manual`, where a person chooses.
    UndefinedStablePolicy,
}

/// What to do about a component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Move desired state to this release unit.
    Advance(Release),

    /// Leave desired state alone.
    Stay(Reason),
}

/// Decides whether an environment advances.
///
/// # It is deliberately this boring
///
/// Five branches, no arithmetic, and nothing that needs a diagram. Everything
/// that could make it complicated has already been decided somewhere better:
///
/// - **"newer than what we run"** is enforced by discovery, which only
///   considers versions sorting strictly after the desired one. So a `newer`
///   at all *is* an upgrade.
/// - **completeness and coherence** are enforced by discovery too. A version
///   still publishing, or built from two commits, never reaches here.
/// - **concurrent changes** are enforced by the write, whose precondition
///   includes the manifest this decision was read from. A decision taken
///   against a manifest that has since gained a hold cannot be applied to it.
///
/// Re-checking any of those here would be a second copy of a rule, and the
/// copies would drift.
///
/// # It cannot change the policy it read
///
/// This returns a decision; it does not write. And what an `Advance` carries
/// is a *version*, so the write it feeds has nothing in it that could clear a
/// hold or widen a policy in order to succeed. The guarantee is structural
/// rather than asserted.
#[must_use]
pub fn decide(policy: UpdatePolicy, channel: Channel, held: bool, discovery: &Discovery) -> Decision {
    match policy {
        UpdatePolicy::Manual => Decision::Stay(Reason::Manual),
        UpdatePolicy::Locked => Decision::Stay(Reason::Locked),
        UpdatePolicy::Automatic if held => Decision::Stay(Reason::Held),

        // Before anything is chosen. What an automatic stable advance may do
        // is undecided, and the safe answer to an undecided rule is to do
        // nothing rather than to do whatever the code happens to permit.
        UpdatePolicy::Automatic if channel == Channel::Stable => {
            Decision::Stay(Reason::UndefinedStablePolicy)
        }
        UpdatePolicy::Automatic => match &discovery.newer {
            Some(release) => Decision::Advance(release.clone()),
            None => Decision::Stay(Reason::NothingNewer),
        },
    }
}
