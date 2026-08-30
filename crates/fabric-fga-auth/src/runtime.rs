//! The runtime surface: the only way a tenant user reaches a decision.
//!
//! ```text
//! POST /v1/check
//! Authorization: Bearer <the user's own realm token>
//!
//! { "relation": "Viewer", "object": "Document:AbC123" }
//! → 200 { "allowed": false }
//! ```
//!
//! # Deliberately tiny
//!
//! One decision route and two health routes. No catch-all, no proxy path, no
//! query-string alternative for identity, tenant, store, relation or object —
//! an endpoint that accepts a value two ways has two things to get right, and
//! the second is the one nobody tests. An unknown path is a `404` from the
//! router, not something forwarded anywhere.
//!
//! # What the response never contains
//!
//! No identity, no tenant, no store, no model, no explanation. The caller
//! asked whether they may do something; `allowed` answers it. Everything else
//! would either echo back trusted state or describe why a check failed, and
//! both are for the log.

mod bearer;
mod health;
#[cfg(test)]
mod runtime_tests;
mod status;

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::{Check, CheckRequest, Decisions, Verifier};

/// The largest request this surface will read.
///
/// A relation and an object. Two kilobytes is roomy for that and refuses a
/// body long before it is deserialised, which is where an unbounded reader
/// becomes a way to spend the process's memory rather than its time.
const MAX_BODY_BYTES: usize = 2_048;

/// What the runtime surface needs to answer.
#[derive(Clone)]
pub struct RuntimeSurface {
    /// Turns a bearer into a verified identity.
    verifier: Arc<Verifier>,

    /// Asks the authorization service about that identity.
    check: Arc<Check>,

    /// The same service, for readiness.
    decisions: Arc<dyn Decisions>,
}

impl RuntimeSurface {
    /// Assembles the surface.
    #[must_use]
    pub const fn new(verifier: Arc<Verifier>, check: Arc<Check>, decisions: Arc<dyn Decisions>) -> Self {
        Self {
            verifier,
            check,
            decisions,
        }
    }

    /// The router, with the body limit applied.
    pub fn router(self) -> Router {
        Router::new()
            .route("/v1/check", post(check))
            .route("/health/live", get(health::live))
            .route("/health/ready", get(health::ready))
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .with_state(self)
    }
}

/// The only thing a caller is told.
#[derive(Serialize)]
struct Decision {
    /// Whether the relation holds.
    allowed: bool,
}

/// `POST /v1/check`.
///
/// The body is deserialised into [`CheckRequest`], whose schema has no `user`,
/// `store` or `tenant` to supply — so binding is structural rather than a step
/// this handler has to remember to perform.
async fn check(
    State(surface): State<RuntimeSurface>,
    headers: HeaderMap,
    body: Result<Json<CheckRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Decision>, StatusCode> {
    // Before the body: a caller with no credential learns nothing about
    // whether their request was well-formed, and neither port is troubled.
    let token = bearer::from(&headers).ok_or(StatusCode::UNAUTHORIZED)?;

    let identity = surface
        .verifier
        .verify(token)
        .await
        .map_err(|error| status::for_verification(&error))?;

    // A body that will not parse, names an unknown field, or carries an
    // invalid relation or object is the caller's mistake and nobody else's.
    //
    // A body refused for its *size* is answered `413` rather than folded in
    // with those. Not pedantry: it is the only way the limit can be told from
    // a parse failure, and a limit whose absence no test can detect is not a
    // limit anybody is maintaining.
    let Json(request) = body.map_err(|rejection| {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            StatusCode::PAYLOAD_TOO_LARGE
        } else {
            StatusCode::BAD_REQUEST
        }
    })?;

    let allowed = surface
        .check
        .run(&identity, &request)
        .await
        .map_err(status::for_decision)?;

    Ok(Json(Decision { allowed }))
}
