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

/// Classifies a status the registry returned.
///
/// `404` never reaches here: a tag that is not published is an *answer*, and
/// the one the whole design rests on — a version missing from one repository
/// is a publishing window, not a failure. It is handled where the request is
/// made.
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
