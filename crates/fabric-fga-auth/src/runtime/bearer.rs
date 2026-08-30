//! Reading the one credential form this surface accepts.

use axum::http::HeaderMap;

/// The scheme, compared case-insensitively as RFC 7235 requires.
const SCHEME: &str = "bearer";

/// The token from an `Authorization` header, if it is exactly a bearer.
///
/// Deliberately strict. There is no query-string alternative, no cookie, no
/// second header, and no other scheme — an endpoint that accepts a credential
/// two ways is an endpoint with two things to get right, and the second one is
/// always the one nobody tests.
pub(super) fn from(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;

    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return None;
    }

    // One space, and something after it. `Bearer  x` has an empty token by
    // this reading, which is what it deserves.
    if token.is_empty() || token.starts_with(' ') {
        return None;
    }

    Some(token)
}
