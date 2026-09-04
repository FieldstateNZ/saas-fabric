//! Bounding one desired-state operation.

use std::future::Future;
use std::time::Duration;

use fabric_platform_management::DesiredStateError;

use crate::PlatformGitRepository;

impl PlatformGitRepository {
    /// Runs one operation under the repository's operation budget.
    ///
    /// # Why the per-request timeout is not enough
    ///
    /// An operation is many requests. `http_timeout_seconds` bounds each of
    /// them separately, so a host that answers every call just inside that
    /// limit keeps the whole operation running for as long as it likes — thirty
    /// calls at ten seconds is five minutes, and a retry round makes it more.
    ///
    /// The platform binding holds its lock across this call, so that is not
    /// merely a slow sweep: an operator's disconnect waits behind it, and the
    /// disconnect request is itself cut off by the API's request timeout long
    /// before an unbounded operation would finish. That failure is silent in the
    /// worst way — the operator is told `504`, the unbind never happened, and
    /// the platform is still pointed at the repository they asked it to forget.
    ///
    /// So every operation the binding delegates to is wrapped here. The drain is
    /// then bounded by construction: the longest an unbind can wait is one
    /// operation budget, which startup has already checked is shorter than a
    /// request.
    pub(super) async fn within_budget<T, F>(&self, operation: F) -> Result<T, DesiredStateError>
    where
        F: Future<Output = Result<T, DesiredStateError>>,
    {
        let budget = Duration::from_secs(self.config.operation_timeout_seconds);

        match tokio::time::timeout(budget, operation).await {
            Ok(outcome) => outcome,
            // Unavailable rather than refused: the repository did not say no,
            // it said nothing, and the caller's next step is to look again
            // rather than to change what they asked for. The detail names the
            // budget and nothing else — no path, no URL, no credential.
            Err(_elapsed) => Err(DesiredStateError::Unavailable {
                detail: format!(
                    "the platform repository did not answer within {} seconds",
                    self.config.operation_timeout_seconds
                ),
            }),
        }
    }
}
