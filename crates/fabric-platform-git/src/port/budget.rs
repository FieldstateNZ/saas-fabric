//! Bounding one desired-state operation, without abandoning a request.

use std::future::Future;
use std::time::{Duration, Instant};

use fabric_platform_management::DesiredStateError;

use crate::{PlatformGitError, PlatformGitRepository};

mod bearer;

tokio::task_local! {
    /// When the operation running on this task began.
    ///
    /// A task-local rather than an argument, because the check belongs at the
    /// one place a request is sent and the thirty calls between the port and
    /// that place have no business carrying a deadline none of them reads. An
    /// operation is a task — the binding spawns one per call — so the task is
    /// exactly the scope the budget applies to.
    ///
    /// Unset means unbudgeted, which is what a test calling a host method
    /// directly gets. Nothing changes for it.
    static STARTED: Instant;
}

impl PlatformGitRepository {
    /// Runs one operation under the repository's operation budget.
    ///
    /// # Why the per-request timeout is not enough
    ///
    /// An operation is many requests. `http_timeout_seconds` bounds each of
    /// them separately, so a host that answers every call just inside that
    /// limit keeps the whole operation running for as long as it likes.
    ///
    /// The platform binding holds its lock across this call, so that is not
    /// merely a slow sweep: an operator's disconnect waits behind it and is
    /// itself cut off by the API's request timeout long before an unbounded
    /// operation would finish. The failure is silent in the worst way — `504`,
    /// the unbind never happened, and the platform still points where it did.
    ///
    /// # The budget never abandons a request the host may act on
    ///
    /// This used to be a [`tokio::time::timeout`], which is the obvious way to
    /// bound an operation and the wrong one. A timeout drops the future, and a
    /// future dropped mid-write releases the binding with its last request
    /// possibly already on the wire — so a disconnect could return and the
    /// abandoned ref update land in a repository the operator had disowned.
    ///
    /// So the budget records when the operation began, and
    /// [`refuse_if_the_budget_is_spent`](Self::refuse_if_the_budget_is_spent)
    /// is consulted before each request is put on the wire. A request that has
    /// started runs to its own outcome under `http_timeout_seconds`; the one
    /// after it is refused instead of being sent. The one exception is obtaining
    /// a bearer, which is bounded rather than gated — the queue inside it is
    /// invisible to a gate on either side, and cutting a token exchange short
    /// writes nothing. See [`bearer_allowance`](Self::bearer_allowance).
    ///
    /// The consequence, which the deployment's timeouts are checked against at
    /// startup: an operation takes at most the budget **plus one request
    /// timeout**, and no write is ever on the wire after the platform gave up.
    ///
    /// # An operation nested in another keeps the outer one's start
    ///
    /// The start already in scope is kept rather than replaced: an operation
    /// composed from two budgeted ones would otherwise be handed a second
    /// budget halfway through, and the bound checked at startup would double.
    ///
    /// # Errors
    ///
    /// Whatever the operation returns, plus [`Unavailable`](DesiredStateError::Unavailable)
    /// when the budget ran out before a request the operation still needed.
    pub(super) async fn within_budget<T, F>(&self, operation: F) -> Result<T, DesiredStateError>
    where
        F: Future<Output = Result<T, DesiredStateError>>,
    {
        let started = STARTED
            .try_with(|started| *started)
            .unwrap_or_else(|_| Instant::now());

        STARTED.scope(started, operation).await
    }

    /// Refuses to start a request the budget no longer covers.
    ///
    /// Called at the one site that sends, twice: once before a bearer is
    /// obtained, because minting one is itself a request to the host, and once
    /// before the call this was building. Nothing already in flight is touched.
    ///
    /// # Errors
    ///
    /// [`Unavailable`](PlatformGitError::Unavailable) once the budget is spent.
    pub(crate) fn refuse_if_the_budget_is_spent(&self) -> Result<(), PlatformGitError> {
        let budget = Duration::from_secs(self.config.operation_timeout_seconds);
        let spent = STARTED
            .try_with(|started| started.elapsed() >= budget)
            .unwrap_or(false);

        if spent {
            return Err(self.out_of_budget());
        }

        Ok(())
    }

    /// The failure an operation that ran out of time is reported as.
    ///
    /// One constructor for both places that give up — the gate before a request
    /// and the bound on acquiring a bearer — because an operator has no use for
    /// the difference. Unavailable rather than refused: the repository did not
    /// say no, it said nothing in time, so the next step is to look again. The
    /// detail names the budget and nothing else — no path, no URL, no credential.
    pub(crate) fn out_of_budget(&self) -> PlatformGitError {
        PlatformGitError::Unavailable {
            detail: format!(
                "the platform repository did not answer inside its {}-second budget",
                self.config.operation_timeout_seconds
            ),
        }
    }
}
