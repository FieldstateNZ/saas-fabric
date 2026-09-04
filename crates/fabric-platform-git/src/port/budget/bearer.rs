//! How long one operation may spend obtaining a bearer, the queue included.

use std::time::Duration;

use super::STARTED;
use crate::PlatformGitRepository;

impl PlatformGitRepository {
    /// What acquiring a bearer is allowed to take: whatever is left of the
    /// budget, plus one request.
    ///
    /// # A check on either side of the mint does not bound the mint
    ///
    /// Under the App posture, `BearerSource::bearer` holds one mutex across the
    /// token exchange on purpose, so that concurrent sweeps share a token
    /// rather than each minting their own. Nothing bounds the *wait* for that
    /// mutex. With an expired token and a token endpoint that stalls, every
    /// operation in turn spends a whole `http_timeout_seconds` failing to mint
    /// before the next one is let in — so a single `attempt` can span several of
    /// them with the binding's read guard held, having passed the check before
    /// the bearer and not yet reached the one after it. On the shipped defaults,
    /// three concurrent operations would make an operator's disconnect wait
    /// thirty seconds against a twenty-five second guarantee, which is the
    /// failure the budget exists to prevent.
    ///
    /// A third check would not help: it would bound the mint and leave the queue
    /// in front of it unbounded, which is where the time actually goes. So the
    /// acquisition as a whole is bounded instead.
    ///
    /// # The arithmetic is the same one the rest of the operation obeys
    ///
    /// The bearer phase ends by the budget plus one request timeout at the
    /// latest; and if it ends inside the budget, the call it was preparing for
    /// takes at most one request timeout of its own. Either way an operation
    /// still ends within its budget plus one request, which is the sum a
    /// deployment's `request_timeout_seconds` is checked against at startup.
    ///
    /// Unbudgeted — a test calling a host method directly — there is no budget
    /// to have anything left of, so the allowance is one request.
    ///
    /// # Why cutting a mint short does not weaken anything
    ///
    /// The rule is that a request the platform has already *sent* is never
    /// abandoned, and it exists for the write path: a ref update dropped
    /// mid-flight releases the binding while the host may still be applying it,
    /// so the platform could report it had stopped writing to a repository and
    /// then write to it. A token exchange has no desired-state side effect at
    /// all. Giving up on one costs a wasted mint, leaves the cache holding
    /// exactly what it held before, and changes nothing in the repository — so
    /// there is nothing here for that rule to protect.
    pub(crate) fn bearer_allowance(&self) -> Duration {
        let budget = Duration::from_secs(self.config.operation_timeout_seconds);

        let remaining = STARTED
            .try_with(|started| budget.saturating_sub(started.elapsed()))
            .unwrap_or(Duration::ZERO);

        remaining.saturating_add(Duration::from_secs(self.config.http_timeout_seconds))
    }
}
