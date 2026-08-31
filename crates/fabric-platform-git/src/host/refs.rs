//! Moving the branch, and the one refusal that means contention.

use reqwest::{Method, StatusCode};

use crate::host::failures::status_failure;
use crate::host::wire::RefUpdateRequest;
use crate::host::PlatformGitRepository;
use crate::{CommitRevision, PlatformGitError};

/// What happened when the branch was asked to move.
///
/// `NotFastForward` is an *outcome*, not an error, and that is the whole
/// design. It is the one signal that means "somebody else committed", and
/// giving it a name keeps it from being inferred from a status code somewhere
/// else — every other failure stays a [`PlatformGitError`] and never enters
/// the retry path.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RefUpdate {
    /// The branch now points at the new commit.
    Applied,

    /// The head had moved, so the update was not a fast-forward.
    NotFastForward,
}

impl PlatformGitRepository {
    /// Points the integration branch at a commit, without forcing.
    ///
    /// `force` is sent as `false` explicitly. The host then requires the update
    /// to be a fast-forward, which is the entire concurrency mechanism: the new
    /// commit's parent is the head that was read, so if the head has moved the
    /// new commit does not contain it and the update is refused.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformGitError`] for every failure *except* the refusal
    /// itself, which comes back as [`RefUpdate::NotFastForward`].
    pub(crate) async fn update_ref(&self, commit: &CommitRevision) -> Result<RefUpdate, PlatformGitError> {
        let branch = &self.config.branch;
        let url = self.url(&format!("git/refs/heads/{branch}"));

        let body = serde_json::to_value(RefUpdateRequest {
            sha: commit.as_str(),
            force: false,
        })
        .map_err(|error| PlatformGitError::Unavailable {
            detail: format!("a ref update could not be encoded: {error}"),
        })?;

        let response = self
            .send("moving the branch", Method::PATCH, url, Some(body))
            .await?;
        let status = response.status();

        if status.is_success() {
            return Ok(RefUpdate::Applied);
        }

        // The one status that means contention. Every other one is a failure,
        // including a `422`: reinterpreting an arbitrary refusal as a race is
        // how a misconfiguration becomes a retry loop.
        if status == StatusCode::CONFLICT {
            return Ok(RefUpdate::NotFastForward);
        }

        Err(status_failure(
            "moving the branch",
            status,
            response.headers(),
            Some(branch),
        ))
    }
}
