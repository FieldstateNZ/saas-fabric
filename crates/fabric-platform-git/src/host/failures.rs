//! Turning transport and status failures into this adapter's vocabulary.

use reqwest::header::HeaderMap;
use reqwest::StatusCode;

use crate::PlatformGitError;

/// The header the host reports remaining quota in.
const RATE_LIMIT_REMAINING: &str = "x-ratelimit-remaining";

/// Builds the right error for a failure from `.send()`.
///
/// The message is this adapter's own classification, never
/// `reqwest::Error`'s `Display`, which can carry the full URL.
///
/// Every transport failure is [`Unavailable`](PlatformGitError::Unavailable),
/// including a timeout that may have fired after the host had already applied
/// the ref update. Such a write is **not** retried inside the call that made
/// it — see `update_files_atomically`, which would otherwise report a conflict
/// against the caller's own change. The caller re-reads instead, finds the
/// change already applied, and has nothing to do.
pub(crate) fn transport_failure(operation: &str, error: &reqwest::Error) -> PlatformGitError {
    let kind = if error.is_connect() {
        "could not connect"
    } else if error.is_timeout() {
        "timed out"
    } else if error.is_decode() {
        "returned a response that could not be read"
    } else {
        "failed"
    };

    PlatformGitError::Unavailable {
        detail: format!("{operation} {kind}"),
    }
}

/// Builds the right error for a status the host returned.
///
/// # `409` is not handled here, on purpose
///
/// A `409` from the ref update is the concurrency signal, and it is read as an
/// outcome by [`update_ref`](super::objects) rather than reaching this
/// function. Nothing else may become a race: a `409` from any other call, and
/// every other status from any call, is a plain failure. Reinterpreting an
/// arbitrary `4xx` as contention is how a misconfiguration turns into a retry
/// loop against a host that will never say yes.
pub(crate) fn status_failure(
    operation: &str,
    status: StatusCode,
    headers: &HeaderMap,
    what: Option<&str>,
) -> PlatformGitError {
    if status == StatusCode::NOT_FOUND {
        return match what {
            Some(what) => PlatformGitError::NotFound {
                what: what.to_owned(),
            },
            None => PlatformGitError::Unavailable {
                detail: format!("{operation} returned 404"),
            },
        };
    }

    if status == StatusCode::FORBIDDEN && quota_exhausted(headers) {
        return PlatformGitError::Unavailable {
            detail: format!("{operation} was rate limited"),
        };
    }

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return PlatformGitError::NotPermitted;
    }

    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        return PlatformGitError::Unavailable {
            detail: format!("{operation} returned {}", status.as_u16()),
        };
    }

    PlatformGitError::Rejected {
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
