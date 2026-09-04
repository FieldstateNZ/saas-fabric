//! How long one operation may spend obtaining a bearer, the queue included.

use std::time::Duration;

use super::STARTED;
use crate::PlatformGitRepository;

impl PlatformGitRepository {
    /// What acquiring a bearer is allowed to take: whatever remains until the
    /// budget plus one request has elapsed since the operation began, or one
    /// request when nothing is budgeted.
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
    /// # Measured to an absolute deadline, not from now
    ///
    /// The allowance is what remains until the budget *plus one request* has
    /// elapsed since the operation began — not one request from whenever this
    /// happens to be asked. The difference matters exactly once the budget is
    /// already spent: a `401` can land up to one request after the budget, and
    /// the invalidation that follows waits on the same mutex. Granted a fresh
    /// request from that moment it would run to the budget plus two, which is
    /// more than startup checked a request can hold. Anchored to the operation's
    /// start it cannot, at any call site, whether or not a gate stood in front.
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
        let one_request = Duration::from_secs(self.config.http_timeout_seconds);
        let bound = Duration::from_secs(self.config.operation_timeout_seconds).saturating_add(one_request);

        STARTED
            .try_with(|started| bound.saturating_sub(started.elapsed()))
            .unwrap_or(one_request)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use fabric_core::SystemClock;
    use fabric_git_host::GitCredential;

    use super::STARTED;
    use crate::{PlatformGitRepository, PlatformRepositoryConfig};

    /// A repository with a one-second budget and a three-second request.
    fn repository() -> PlatformGitRepository {
        PlatformGitRepository::new(
            &PlatformRepositoryConfig {
                api_base_url: "http://127.0.0.1:1".to_owned(),
                owner: "o".to_owned(),
                repository: "r".to_owned(),
                branch: "main".to_owned(),
                http_timeout_seconds: 3,
                operation_timeout_seconds: 1,
            },
            GitCredential::token("test-bearer"),
            Arc::new(SystemClock),
        )
        .expect("a valid configuration")
    }

    /// The allowance as seen `elapsed` into an operation.
    async fn allowance_after(elapsed: Duration) -> Duration {
        let repository = repository();
        let started = Instant::now().checked_sub(elapsed).expect("a recent instant");

        STARTED
            .scope(started, async move { repository.bearer_allowance() })
            .await
    }

    #[test]
    fn unbudgeted_the_allowance_is_one_request() {
        assert_eq!(repository().bearer_allowance(), Duration::from_secs(3));
    }

    #[tokio::test]
    async fn the_allowance_is_what_is_left_until_the_budget_plus_one_request() {
        // Two seconds in, with a bound of four, two remain -- not a fresh
        // three measured from now, which is what a `401` landing after the
        // budget would otherwise be handed.
        let allowance = allowance_after(Duration::from_secs(2)).await;

        assert!(
            allowance > Duration::from_millis(1900) && allowance <= Duration::from_secs(2),
            "{allowance:?}"
        );
    }

    #[tokio::test]
    async fn past_the_bound_nothing_is_allowed() {
        assert_eq!(allowance_after(Duration::from_secs(5)).await, Duration::ZERO);
    }
}
