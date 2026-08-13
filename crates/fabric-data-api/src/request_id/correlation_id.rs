//! Choosing the correlation id for one request, and the rules a
//! caller-supplied one has to satisfy to be adopted.
//!
//! Whatever this module returns is reflected three ways: onto the response
//! header, into the error body, and into the tracing fields of every event
//! recording the failure. Tracing output here is JSON, so a stray newline
//! cannot forge a log line — but nothing used to bound the *size*, and an
//! unbounded value reflected on every line it touches is a caller-controlled
//! amplifier on this service's log volume and response size.

use axum::http::{HeaderMap, HeaderName, HeaderValue};

#[cfg(test)]
mod correlation_id_tests;

/// The header a caller may set to propagate its own id, and that this crate
/// always sets on the way out.
pub(super) const HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// The longest inbound id this crate will adopt.
///
/// Comfortably clear of every id format in real use — a UUID is 36
/// characters, a W3C `traceparent` 55, an X-Ray trace id 35 — while keeping
/// the worst case a caller can impose on one log line, and on the error body,
/// to something bounded and boring. A value past this length is not a
/// correlation id; it is a payload wearing one's clothes.
const MAX_LENGTH: usize = 128;

/// The inbound `X-Request-Id` if the caller sent one worth adopting; a fresh
/// id otherwise.
///
/// An acceptable inbound id is propagated rather than replaced so a caller's
/// own tracing — or a gateway in front of this service — and this crate's logs
/// share one id for the same request.
///
/// # Why an unacceptable id is replaced rather than truncated
///
/// Truncating would hand back something that *looks* like the caller's id but
/// is not. The id they quote to an operator would no longer be the id in the
/// log, and two callers whose ids share a long prefix would silently collapse
/// onto one value — merging two requests' traces, which is worse than not
/// correlating them at all. A generated id is honestly different, and the
/// caller can see that it is different, because it comes back on the response
/// header. Propagation is a convenience; a correct id is not.
pub(super) fn for_request(headers: &HeaderMap) -> String {
    headers
        .get(HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| acceptable(value))
        .map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_owned)
}

/// Whether a caller-supplied id may be adopted as this request's id.
///
/// Bounded, non-empty, and drawn from the characters that id formats actually
/// use: alphanumerics plus the punctuation found in UUIDs, W3C trace context,
/// X-Ray ids, and base64-shaped gateway ids. Everything else is refused,
/// including spaces, tabs, and every control character.
///
/// An allowlist rather than a blocklist, because this value's destinations
/// change over time — today a header, a JSON body, and JSON log fields;
/// tomorrow perhaps a metrics label or a trace attribute with escaping rules
/// nobody checked. A list of permitted characters stays correct when a
/// destination is added; a list of characters that broke something once does
/// not. The length test runs first, so a megabyte argument costs one
/// comparison rather than a megabyte of iteration.
fn acceptable(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_LENGTH
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '+' | '/' | '='))
}

/// Encodes an id chosen by [`for_request`] as a header value.
///
/// Infallible by construction: [`acceptable`] admits only visible ASCII, and a
/// generated UUID is hexadecimal. The fallback exists because [`HeaderValue`]
/// offers no constructor that can say so in the type system, and a response
/// without a correlation id would break the promise
/// [`data_routes`](crate::data_routes) makes.
pub(super) fn header_value(id: &str) -> HeaderValue {
    HeaderValue::from_str(id).unwrap_or_else(|_| HeaderValue::from_static("unencodable-request-id"))
}
