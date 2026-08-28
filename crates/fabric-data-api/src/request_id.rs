//! A per-request correlation id, carried without threading a parameter
//! through every function between the router and an error response.
//!
//! When this crate can only tell a caller "internal error", this id is what
//! lets them turn that into something an operator can find: quote the id,
//! and the matching log event is the line with the detail the response
//! deliberately withheld (§29).
//!
//! Which id a request gets — and why a caller-supplied one is bounded and
//! character-checked before it is adopted — is [`correlation_id`]'s job. This
//! module is only the plumbing that scopes it and puts it on the response.
//!
//! # Why task-local rather than a parameter
//!
//! The id is produced once, in [`middleware`], before the router is even
//! entered. Passing it explicitly to whatever eventually builds a
//! [`DataApiError`](crate::DataApiError) response would mean adding a
//! parameter to every method between a handler and
//! `DataApiError::into_response` — `list`, `read`, `create`, `update`,
//! `delete`, `prepare`, `execute_query`, `execute_mutation` — for a value only
//! the failure path ever reads. A [`tokio::task_local!`] scopes the id to
//! exactly the future processing this request, which is the same lifetime a
//! parameter would have had, without the plumbing.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

mod correlation_id;

#[cfg(test)]
mod request_id_tests;

tokio::task_local! {
    static CURRENT: String;
}

/// Establishes the request id for the lifetime of one request, and sets it on
/// the response header regardless of how the request was handled.
///
/// Applied once, at [`data_routes`](crate::data_routes), so no handler can
/// forget it and no response can go out without it. The header is written
/// unconditionally for that reason —
/// [`correlation_id::header_value`] cannot fail to produce one.
pub(crate) async fn middleware(request: Request, next: Next) -> Response {
    let id = correlation_id::for_request(request.headers());
    let header = correlation_id::header_value(&id);

    let mut response = CURRENT.scope(id, next.run(request)).await;
    response.headers_mut().insert(correlation_id::HEADER, header);

    response
}

/// The current request's id, for tagging a failure response and its log
/// event with the same value.
///
/// Falls back to a fixed marker outside a request context — a unit test
/// constructing a [`DataApiError`](crate::DataApiError) directly, say — rather
/// than panicking: nothing about producing an error response should be able to
/// fail.
pub(crate) fn current() -> String {
    CURRENT
        .try_with(Clone::clone)
        .unwrap_or_else(|_| "no-request-context".to_owned())
}
