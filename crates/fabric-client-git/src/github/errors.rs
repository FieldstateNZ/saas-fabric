//! Turning transport and status failures into repository errors.

use fabric_client_model::ClientId;
use fabric_control_plane::RepositoryError;
use reqwest::header::HeaderMap;
use reqwest::StatusCode;

/// The header the host reports remaining quota in.
const RATE_LIMIT_REMAINING: &str = "x-ratelimit-remaining";

/// Builds the right error for a failure from `.send()`.
///
/// Every transport failure is [`Unavailable`](RepositoryError::Unavailable),
/// including a timeout that may have fired after the host had already
/// committed. That is safe here for a reason specific to this API: the write
/// is conditional on a blob hash, so a retry of a write that actually
/// succeeded is refused as a conflict rather than applied twice. The operator
/// is told to re-read and redo, which is correct — the change did land.
///
/// The message is this adapter's own classification, never
/// `reqwest::Error`'s `Display`, which can carry the full URL.
pub(super) fn transport_failure(operation: &str, error: &reqwest::Error) -> RepositoryError {
    let kind = if error.is_connect() {
        "could not connect"
    } else if error.is_timeout() {
        "timed out"
    } else if error.is_decode() {
        "returned a response that could not be read"
    } else {
        "failed"
    };

    RepositoryError::Unavailable {
        detail: format!("{operation} {kind}"),
    }
}

/// Builds the right error for a status the host returned.
///
/// # The cases that are not obvious
///
/// - **`403` with no quota left** is a rate limit, which is transient, and
///   reporting it as a refused credential would send an operator looking for a
///   secret that is perfectly fine. The header is what distinguishes them.
/// - **`409` and `422`** both mean the write's precondition did not hold: the
///   host uses one for a stale blob hash and the other for related conflicts
///   on the same file. Both are a lost race, and both must produce the same
///   answer — re-read and redo.
/// - **Other `4xx`** are the platform asking for something the host will not
///   do, which no retry fixes. Reporting them as `Unavailable` would invite a
///   retry loop over a misconfiguration (§23).
pub(super) fn status_failure(
    operation: &str,
    status: StatusCode,
    headers: &HeaderMap,
    client: Option<&ClientId>,
) -> RepositoryError {
    if status == StatusCode::NOT_FOUND {
        return match client {
            Some(client) => RepositoryError::NotFound {
                client: client.clone(),
            },
            None => RepositoryError::Unavailable {
                detail: format!("{operation} returned 404"),
            },
        };
    }

    if status == StatusCode::FORBIDDEN && quota_exhausted(headers) {
        return RepositoryError::Unavailable {
            detail: format!("{operation} was rate limited"),
        };
    }

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return RepositoryError::NotPermitted;
    }

    if status == StatusCode::CONFLICT || status == StatusCode::UNPROCESSABLE_ENTITY {
        return RepositoryError::Conflict;
    }

    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        return RepositoryError::Unavailable {
            detail: format!("{operation} returned {}", status.as_u16()),
        };
    }

    RepositoryError::Rejected {
        detail: format!("{operation} was refused with {}", status.as_u16()),
    }
}

/// Whether the host reported no remaining quota.
fn quota_exhausted(headers: &HeaderMap) -> bool {
    headers
        .get(RATE_LIMIT_REMAINING)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == "0")
}
