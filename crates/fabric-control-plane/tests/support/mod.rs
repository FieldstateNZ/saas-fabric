//! A control plane assembled the way the host assembles it.
//!
//! These tests drive the real router — the one `build_control_plane` returns —
//! rather than a hand-assembled copy. A test that rebuilds the thing it is
//! checking is checking itself, and the missing piece is always the one that
//! mattered: here it would be the operator extractor, whose absence would make
//! every endpoint public and every test still pass.

// Each test binary compiles the whole support module but uses a subset of it,
// so unused items here are expected rather than a smell.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::Router;
use fabric_client_model::{ClientDocument, ClientRevision};
use fabric_control_plane::{build_control_plane, ControlPlaneConfig, InMemoryClientRepository};
use fabric_core::Clock;
use fabric_reconciliation::ReconciliationStatusStore;
use http::{header, Request, Response};
use tower::ServiceExt as _;

/// The operator every test authenticates as.
pub const OPERATOR: &str = "brett@example.com";

/// The header the configured posture reads.
pub const OPERATOR_HEADER: &str = "Tailscale-User-Login";

/// A client document with a section the control plane does not model.
pub const ACME: &str = r"
apiVersion: fabric.fieldstate.nz/v1
kind: Client
metadata:
  name: acme
spec:
  displayName: Acme
  hosts:
    - www.example.com
  identity:
    realm: acme
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients:
      - id: web
        type: oidc
        redirectUris:
          - https://www.example.com/callback
  features:
    invoicing: true
";

/// A clock that never moves.
pub struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_700_000_000
    }
}

/// The router, plus what a test needs to set the world up around it.
pub struct TestControlPlane {
    /// The router the host would serve.
    pub router: Router,

    /// The desired state behind it.
    pub repository: Arc<InMemoryClientRepository>,

    /// The revision `acme` is currently at.
    pub revision: ClientRevision,

    /// What the control plane believes about reconciliation.
    ///
    /// Exposed so a test can put a client into a state only the reconciliation
    /// loop could otherwise produce — a failed pass, say — without running a
    /// loop.
    pub statuses: Arc<ReconciliationStatusStore>,
}

/// Builds a control plane holding one client.
pub fn control_plane() -> TestControlPlane {
    let repository = Arc::new(InMemoryClientRepository::new());
    let revision = repository
        .insert(ClientDocument::parse(ACME).expect("the fixture document must parse"))
        .expect("the fixture must store");

    let config: ControlPlaneConfig = serde_json::from_value(serde_json::json!({
        "operator": {
            "mode": "trusted_header",
            "header": OPERATOR_HEADER,
            "allowlist": [OPERATOR],
        }
    }))
    .expect("the fixture configuration must load");

    let services = build_control_plane(&config, repository.clone(), Arc::new(FixedClock))
        .expect("the control plane must build");

    TestControlPlane {
        router: services.router,
        repository,
        revision,
        statuses: services.statuses,
    }
}

/// Sends a request and returns the response.
pub async fn send(router: &Router, request: Request<Body>) -> Response<Body> {
    router
        .clone()
        .oneshot(request)
        .await
        .expect("the router must answer")
}

/// A request as an authenticated operator.
pub fn as_operator(method: &str, path: &str) -> http::request::Builder {
    Request::builder()
        .method(method)
        .uri(path)
        .header(OPERATOR_HEADER, OPERATOR)
}

/// Reads a response body as JSON.
pub async fn json(response: Response<Body>) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("the body must be readable");

    serde_json::from_slice(&bytes).expect("the body must be JSON")
}

/// The `ETag` a response carried, unquoted.
pub fn entity_tag(response: &Response<Body>) -> String {
    response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"').to_owned())
        .expect("the response must carry an entity tag")
}
