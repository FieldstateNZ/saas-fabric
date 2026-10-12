//! The separate listener a control plane reads this replica's served
//! generation from (G7). Off unless `reader_generation_listen` is set.
//!
//! # What it answers, and what it never does
//!
//! One route, `GET /reader-generation`, answering only the generation
//! number, the publication record's SHA-256 and whether the copy came from
//! the published files or the last-known-good copy. They are read with one
//! load from the view resolution itself reads
//! ([`RuntimeResolver::served_generation`]), so the answer is the installed
//! snapshot's, never a configured or mounted generation. Nothing that names
//! a tenant, an issuer, a data source, a path or a token is reachable here.
//!
//! The listener is unauthenticated, which is why it is separate: the main
//! listener's probes keep their admin-gated detail and their minimal body,
//! and the Data API gains no route. Whether anything may reach this port is
//! a deployment decision (traffic isolation by a `NetworkPolicy`, which is
//! not proof of who answered) that has not been approved. Nothing in this
//! repository sets it.

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use fabric_core::READER_GENERATION_PATH;
use fabric_tenant_runtime::RuntimeResolver;
use http::header::{CACHE_CONTROL, CONTENT_TYPE};
use http::StatusCode;

use crate::config::AppConfig;

/// The listener to bind beside the main one, when one is configured.
pub struct ReaderGenerationListener {
    /// The address to bind: `reader_generation_listen`, as validated.
    pub address: String,
    /// What it serves: [`router`] and nothing else.
    pub router: Router,
}

impl ReaderGenerationListener {
    /// The listener `config` asks for, over `runtime`; `None`, binding
    /// nothing, unless `reader_generation_listen` is set.
    #[must_use]
    pub fn configured(config: &AppConfig, runtime: &Arc<RuntimeResolver>) -> Option<Self> {
        config.reader_generation_listen.as_ref().map(|address| Self {
            address: address.clone(),
            router: router(Arc::clone(runtime)),
        })
    }
}

/// The listener's whole surface: `GET` [`READER_GENERATION_PATH`].
///
/// `200` with the report while the installed view holds a whole recorded
/// generation; `404` with no body otherwise (unprimed, documents with no
/// record, or a generation a registry did not take whole), which a reader of
/// the report treats as nothing observed.
pub fn router(runtime: Arc<RuntimeResolver>) -> Router {
    Router::new()
        .route(READER_GENERATION_PATH, get(served))
        .with_state(runtime)
}

async fn served(State(runtime): State<Arc<RuntimeResolver>>) -> Response {
    let Some(report) = runtime.served_generation() else {
        return (StatusCode::NOT_FOUND, [(CACHE_CONTROL, "no-store")]).into_response();
    };
    match serde_json::to_vec(&report) {
        Ok(body) => (
            [(CONTENT_TYPE, "application/json"), (CACHE_CONTROL, "no-store")],
            body,
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
