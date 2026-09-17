//! A create is sent with no `sha`, so it has nothing to be stale against —
//! `422` on a create means something [`status_failure`](super::status_failure)'s
//! `422` does not.

use fabric_client_model::ClientId;
use fabric_control_plane::RepositoryError;
use reqwest::header::HeaderMap;
use reqwest::StatusCode;

use super::status_failure;

/// Builds the right error for a status the host returned on a *create* —
/// a write sent with no `sha`, because there is nothing yet to be stale
/// against.
///
/// # Why `422` is not `status_failure`'s `422`
///
/// `status_failure` treats `409` and `422` as the same event because, on an
/// *update*, they genuinely are: a stale blob hash and a conflicting write
/// against the same file are both a lost race, answered the same way. A
/// create has no blob hash to be stale against, so GitHub's `422` there
/// means one of two different things instead: the file already exists (a
/// create with no `sha` supplied has nothing to overwrite it with), or the
/// request itself was invalid — a path or encoding GitHub refuses outright.
/// Neither is `409`'s branch race, and reporting either as
/// [`RepositoryError::Conflict`] would tell `create_client` a write that
/// can never succeed is worth retrying. [`RepositoryError::Rejected`]
/// instead: not retryable, which is exactly right for a genuine validation
/// failure, and merely imprecise for "the file already exists" — a case
/// `create_client` resolves the same way regardless, by reading the id back
/// (see its own rustdoc).
pub(in crate::github) fn create_status_failure(
    operation: &str,
    status: StatusCode,
    headers: &HeaderMap,
    client: Option<&ClientId>,
) -> RepositoryError {
    if status == StatusCode::UNPROCESSABLE_ENTITY {
        return RepositoryError::Rejected {
            detail: format!("{operation} was refused with {}", status.as_u16()),
        };
    }

    status_failure(operation, status, headers, client)
}
