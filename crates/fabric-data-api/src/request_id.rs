//! A per-request correlation id, carried without threading a parameter
//! through every function between the router and an error response.
//!
//! When this crate can only tell a caller "internal error", this id is what
//! lets them turn that into something an operator can find: quote the id,
//! and the matching log event is the line with the detail the response
//! deliberately withheld (§29).
//!
//! # Why task-local rather than a parameter
//!
//! The id is produced once, in [`middleware`], before the router is even
//! entered. Passing it explicitly to whatever eventually builds a
//! [`DataApiError`] response would mean adding a parameter to every method
//! between a handler and `DataApiError::into_response` — `list`, `read`,
//! `create`, `update`, `delete`, `prepare`, `execute_query`,
//! `execute_mutation` — for a value only the failure path ever reads. A
//! [`tokio::task_local!`] scopes the id to exactly the future processing
//! this request, which is the same lifetime a parameter would have had,
//! without the plumbing.

use axum::extract::Request;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

/// The header a caller may set to propagate its own id, and that this crate
/// always echoes back.
const HEADER: HeaderName = HeaderName::from_static("x-request-id");

tokio::task_local! {
    static CURRENT: String;
}

/// Establishes the request id for the lifetime of one request, and echoes it
/// on the response header regardless of how the request was handled.
///
/// Applied once, at [`data_routes`](crate::data_routes), so no handler can
/// forget it and no response can go out without it.
pub(crate) async fn middleware(request: Request, next: Next) -> Response {
    let id = inbound_or_generated(request.headers());
    let mut response = CURRENT.scope(id.clone(), next.run(request)).await;

    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(HEADER, value);
    }

    response
}

/// The inbound `X-Request-Id`, if the caller sent one and it is usable as a
/// header value; a fresh id otherwise.
///
/// An inbound id is propagated rather than replaced so a caller's own
/// tracing — or a gateway in front of this service — and this crate's logs
/// share one id for the same request.
fn inbound_or_generated(headers: &HeaderMap) -> String {
    headers
        .get(HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_owned)
}

/// The current request's id, for tagging a failure response and its log
/// event with the same value.
///
/// Falls back to a fixed marker outside a request context — a unit test
/// constructing a [`DataApiError`] directly, say — rather than panicking:
/// nothing about producing an error response should be able to fail.
pub(crate) fn current() -> String {
    CURRENT
        .try_with(Clone::clone)
        .unwrap_or_else(|_| "no-request-context".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_a_request_the_current_id_is_a_safe_marker() {
        assert_eq!(current(), "no-request-context");
    }

    #[test]
    fn an_absent_header_produces_a_generated_id() {
        let headers = HeaderMap::new();

        assert!(uuid::Uuid::parse_str(&inbound_or_generated(&headers)).is_ok());
    }

    #[test]
    fn an_inbound_header_is_propagated_unchanged() {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER, HeaderValue::from_static("caller-supplied-id"));

        assert_eq!(inbound_or_generated(&headers), "caller-supplied-id");
    }

    #[test]
    fn an_empty_inbound_header_is_treated_as_absent() {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER, HeaderValue::from_static(""));

        assert!(uuid::Uuid::parse_str(&inbound_or_generated(&headers)).is_ok());
    }

    #[tokio::test]
    async fn the_scoped_id_is_readable_for_the_life_of_the_future() {
        let seen = CURRENT.scope("scoped-id".to_owned(), async { current() }).await;

        assert_eq!(seen, "scoped-id");
    }
}
