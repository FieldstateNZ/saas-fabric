//! Turning transport and status failures into the port's vocabulary.

use fabric_platform_management::RegistryError;

/// The header a registry reports remaining quota in.
const RATE_LIMIT_REMAINING: &str = "x-ratelimit-remaining";

/// Classifies a failure from `.send()`.
///
/// The message is this adapter's own classification, never
/// `reqwest::Error`'s `Display`, which can carry the full URL.
pub(crate) fn transport_failure(operation: &str, error: &reqwest::Error) -> RegistryError {
    let kind = if error.is_connect() {
        "could not connect"
    } else if error.is_timeout() {
        "timed out"
    } else if error.is_decode() {
        "returned a response that could not be read"
    } else {
        "failed"
    };

    RegistryError::Unavailable {
        detail: format!("{operation} {kind}"),
    }
}

/// Classifies a failure from `.send()` on a client whose redirect policy may
/// refuse a hop.
///
/// A policy refusal is not an outage: the far end was reachable and this
/// crate's own transport policy said no to where it was sent next. Grouping
/// it with `Unavailable` would tell an operator to retry a request that will
/// refuse again. `reqwest` keeps the reason the policy raised as the error's
/// `source()`, worded by the reader that refused, so that is what is shown.
pub(crate) fn send_failure(operation: &str, error: &reqwest::Error) -> RegistryError {
    if !error.is_redirect() {
        return transport_failure(operation, error);
    }

    let detail = std::error::Error::source(error).map_or_else(
        || format!("{operation} was redirected off the allowed transport"),
        ToString::to_string,
    );

    RegistryError::Refused { detail }
}

/// A body that arrived whole, within its bound, and is not the document it
/// should have been.
///
/// `Unavailable`, as a body `reqwest` could not decode always was here: a
/// listing or token response is not content-addressed, and the next pass
/// asks again. Content that *is* addressed by a digest, and hashed, is
/// judged where it is read instead.
pub(crate) fn unreadable(operation: &str) -> RegistryError {
    RegistryError::Unavailable {
        detail: format!("{operation} returned a response that could not be read"),
    }
}

/// Classifies a status the registry returned.
///
/// A `404` that is an *answer* never reaches here: a tag that is not
/// published is one, and the one the whole design rests on — a version
/// missing from one repository is a publishing window, not a failure. It is
/// handled where the request is made. A `404` that is not an answer — a
/// later page of a listing, or the token endpoint — does reach here, and is
/// refused like any other status.
///
/// A `429` or a `403` with no quota left is a rate limit, which is transient
/// and leaves availability stale rather than wrong.
pub(crate) fn status_failure(
    operation: &str,
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
) -> RegistryError {
    let rate_limited = status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || (status == reqwest::StatusCode::FORBIDDEN && quota_exhausted(headers));

    if rate_limited || status.is_server_error() {
        return RegistryError::Unavailable {
            detail: format!("{operation} returned {}", status.as_u16()),
        };
    }

    RegistryError::Refused {
        detail: format!("{operation} was refused with {}", status.as_u16()),
    }
}

/// Whether the registry reported no remaining quota.
fn quota_exhausted(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get(RATE_LIMIT_REMAINING)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == "0")
}
