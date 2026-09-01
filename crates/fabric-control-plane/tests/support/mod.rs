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
use fabric_control_plane::testing::AcceptingOperator;
use fabric_control_plane::{
    build_control_plane, ControlPlaneConfig, ControlPlaneDeps, DesiredStateBinding, InMemoryClientRepository,
};
use fabric_core::Clock;
use fabric_reconciliation::ReconciliationStatusStore;
use http::{header, Request, Response};
use tower::ServiceExt as _;

/// The operator every test authenticates as.
pub const OPERATOR: &str = "brett@example.com";

/// A header these tests still set, so that "authenticated" is visible in each
/// request rather than implied by the harness.
///
/// The authenticator ignores it — see [`AcceptingOperator`]. What it preserves
/// is the shape of a test: a request that omits it is written as an anonymous
/// one, and the tests that assert `401` say so by building the request without
/// this rather than by reaching into the harness.
pub const OPERATOR_HEADER: &str = fabric_control_plane::testing::TEST_OPERATOR_HEADER;

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
  secrets:
    namespace: acme
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

    /// The binding behind the router, so a test can connect or disconnect
    /// desired state while the control plane is running — which is what an
    /// operator does, and what no restart-based test would exercise.
    pub binding: Arc<DesiredStateBinding>,
}

/// A stand-in for a client's secret store.
///
/// Holds one secret so that a test can exercise the routes without a store,
/// and — more to the point — so a test can assert what the *response* carries
/// rather than what the adapter did.
pub struct FakeClientSecrets;

#[async_trait::async_trait]
impl fabric_control_plane::ClientSecrets for FakeClientSecrets {
    async fn list(
        &self,
        _namespace: &fabric_control_plane::SecretNamespace,
    ) -> Result<Vec<fabric_control_plane::SecretPath>, fabric_control_plane::SecretsError> {
        Ok(vec![
            fabric_control_plane::SecretPath::parse("database/primary").expect("valid")
        ])
    }

    async fn metadata(
        &self,
        _namespace: &fabric_control_plane::SecretNamespace,
        _path: &fabric_control_plane::SecretPath,
    ) -> Result<fabric_control_plane::SecretMetadata, fabric_control_plane::SecretsError> {
        Ok(fabric_control_plane::SecretMetadata {
            version: 7,
            updated_at: Some("2026-08-30T00:00:00Z".to_owned()),
        })
    }

    async fn reveal(
        &self,
        _namespace: &fabric_control_plane::SecretNamespace,
        _path: &fabric_control_plane::SecretPath,
    ) -> Result<fabric_control_plane::SecretValues, fabric_control_plane::SecretsError> {
        Ok(fabric_control_plane::SecretValues::new(
            [("password".to_owned(), SECRET_VALUE.to_owned())]
                .into_iter()
                .collect(),
        ))
    }

    async fn write(
        &self,
        _namespace: &fabric_control_plane::SecretNamespace,
        _path: &fabric_control_plane::SecretPath,
        _values: &fabric_control_plane::SecretValues,
        expected: Option<u64>,
    ) -> Result<u64, fabric_control_plane::SecretsError> {
        // Anything but the current version is somebody else having written
        // first, which is the case worth being able to reach from a test.
        if expected == Some(7) {
            Ok(8)
        } else {
            Err(fabric_control_plane::SecretsError::Conflict)
        }
    }

    async fn delete(
        &self,
        _namespace: &fabric_control_plane::SecretNamespace,
        _path: &fabric_control_plane::SecretPath,
    ) -> Result<(), fabric_control_plane::SecretsError> {
        Ok(())
    }
}

/// The value the fake holds, so a test can assert where it does and does not
/// appear.
pub const SECRET_VALUE: &str = "a-value-that-must-not-leak";

/// Builds a control plane holding one client.
pub fn control_plane() -> TestControlPlane {
    let repository = Arc::new(InMemoryClientRepository::new());
    let revision = repository
        .insert(ClientDocument::parse(ACME).expect("the fixture document must parse"))
        .expect("the fixture must store");

    let config: ControlPlaneConfig = serde_json::from_value(serde_json::json!({
        "operator": {
            "mode": "oidc",
            "issuer": "https://auth.example.test/realms/master",
            "redirect_uri": "https://fabric.example.test/",
        }
    }))
    .expect("the fixture configuration must load");

    let binding = DesiredStateBinding::to(repository.clone());

    let services = build_control_plane(
        &config,
        ControlPlaneDeps {
            // Nothing connected, which is what the platform route reports.
            platform: None,
            client_secrets: Some(Arc::new(FakeClientSecrets)),
            desired_state: Arc::clone(&binding),
            clock: Arc::new(FixedClock),
            keys: fabric_control_plane::KeyHolder::empty(),
            identity_provider: None,
            sign_in: None,
            git_integration: None,

            // The posture is verified by its own tests. These drive everything
            // above it, and minting signed tokens here would make every one of
            // them a test about authentication.
            operators: Some(AcceptingOperator::accepting(OPERATOR)),
        },
    )
    .expect("the control plane must build");

    TestControlPlane {
        router: services.router,
        repository,
        revision,
        statuses: services.statuses,
        binding,
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
