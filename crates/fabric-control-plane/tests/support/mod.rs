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
    build_control_plane, ControlPlaneConfig, ControlPlaneDeps, DesiredStateBinding, IdentityProviderFactory,
    InMemoryClientRepository, OperatorToken, PlatformBinding, PublicationSink,
};
use fabric_core::Clock;
use fabric_reconciliation::testing::FakeIdentityProvider;
use fabric_reconciliation::{IdentityProvider, ReconciliationStatusStore};
use http::{header, Request, Response};
use tower::ServiceExt as _;

pub mod platform_fixture;

/// The operator every test authenticates as.
pub const OPERATOR: &str = "brett@example.com";

/// The realm the fixture operator posture authenticates against — distinct
/// from `master`, so a test can tell "reserved because it is `master`" apart
/// from "reserved because it is the operator's own realm".
pub const OPERATOR_REALM: &str = "platform-operators";

/// An application id this deployment's own console is fixed under, the way
/// a real composition root computes `reserved_client_ids` from
/// configuration — see `fabric-control-plane-api`'s
/// `startup::reserved_names::client_ids`. Named here rather than left an
/// empty set so a test can drive the real router against a genuine
/// deployment-reserved id, not only the static built-ins
/// `fabric-client-model` refuses on its own.
pub const RESERVED_APPLICATION_ID: &str = "platform-console";

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
    build(None, None, None)
}

/// Builds a control plane holding one client, converging against `provider`
/// when an operator asks it to.
///
/// Every other test in this crate asserts against `plane.statuses` directly,
/// which is cheaper when nothing needs a real reconciliation pass to have
/// happened. The composed proof does need one — it drives `POST
/// /api/reconciliation`, the real door an operator uses — so it is the one
/// caller of this function.
pub fn control_plane_with_identity_provider(provider: Arc<FakeIdentityProvider>) -> TestControlPlane {
    build(Some(Arc::new(FakeIdentityProviderFactory(provider))), None, None)
}

/// Builds a control plane with a platform bound, for tests that drive
/// `/api/platform/*` against something other than "nothing is managed".
pub fn control_plane_with_platform(platform: PlatformBinding) -> TestControlPlane {
    build(None, Some(platform), None)
}

/// Builds a control plane with a platform bound *and* somewhere to publish
/// to, for tests that drive `POST /api/platform/publication` and the
/// `publication` row `GET /api/platform` renders (ADR 0023 part 4) against
/// something other than "nothing is configured".
pub fn control_plane_with_publication(platform: PlatformBinding, sink: PublicationSink) -> TestControlPlane {
    build(None, Some(platform), Some(sink))
}

/// Shared by every constructor above; only what lends the identity
/// provider's authority, which platform is bound, and where it publishes
/// differ between them.
fn build(
    identity_provider: Option<Arc<dyn IdentityProviderFactory>>,
    platform: Option<PlatformBinding>,
    publication: Option<PublicationSink>,
) -> TestControlPlane {
    let repository = Arc::new(InMemoryClientRepository::new());
    let revision = repository
        .insert(&ClientDocument::parse(ACME).expect("the fixture document must parse"))
        .expect("the fixture must store");

    let config: ControlPlaneConfig = serde_json::from_value(serde_json::json!({
        "operator": {
            "mode": "oidc",
            "issuer": format!("https://auth.example.test/realms/{OPERATOR_REALM}"),
            "redirect_uri": "https://fabric.example.test/",
        }
    }))
    .expect("the fixture configuration must load");

    let binding = DesiredStateBinding::to(repository.clone());

    // What a real composition root computes and hands in — see
    // `fabric-control-plane-api`'s `startup::application::build`. Named here
    // rather than left empty so `create_client`'s realm-reservation rule has
    // something real to refuse in a test that drives the real router.
    let reserved_realms = ["master".to_owned(), OPERATOR_REALM.to_owned()]
        .into_iter()
        .collect();

    let services = build_control_plane(
        &config,
        ControlPlaneDeps {
            platform,
            platform_integration: None,
            publication,
            client_secrets: Some(Arc::new(FakeClientSecrets)),
            desired_state: Arc::clone(&binding),
            clock: Arc::new(FixedClock),
            keys: fabric_control_plane::KeyHolder::empty(),
            identity_provider,
            sign_in: None,
            git_integration: None,

            // The posture is verified by its own tests. These drive everything
            // above it, and minting signed tokens here would make every one of
            // them a test about authentication.
            operators: Some(AcceptingOperator::accepting(OPERATOR)),

            reserved_realms,
            reserved_client_ids: [RESERVED_APPLICATION_ID.to_owned()].into_iter().collect(),
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

/// Lends the same fake identity provider to every operator.
///
/// A real deployment builds one provider per operator, from the bearer they
/// presented (`crate::identity_authority`'s whole reason for existing). These
/// tests have one operator and one fake behind it, so "acting as" ignores the
/// authority it is handed and returns the same fake every time.
struct FakeIdentityProviderFactory(Arc<FakeIdentityProvider>);

impl IdentityProviderFactory for FakeIdentityProviderFactory {
    fn acting_as(&self, _authority: &OperatorToken) -> Arc<dyn IdentityProvider> {
        self.0.clone()
    }

    fn describe(&self) -> String {
        "a fake identity provider for tests".to_owned()
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

/// A directory under the system temp root, unique per test, removed when it
/// drops. `tempfile` is not in this workspace's dependency table -- see
/// `fabric-runtime-publication/tests/filesystem_runtime_publication.rs`,
/// whose own copy this mirrors, for the same reason.
pub struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        let unique = format!(
            "fabric-control-plane-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the clock must be after the epoch")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("a temp directory must be creatable");
        Self { path }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
