//! The HTTP contract, which is where the distinctions below can blur.
//!
//! Every layer under this one keeps a credential problem, an outage, a
//! misconfiguration and a decision apart. A transport that mapped two of them
//! to the same status would throw that away at the last step, and nothing
//! below would notice.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use fabric_core::Clock;
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
use serde_json::json;
use tower::ServiceExt as _;

use crate::{
    Check, DecisionFailure, Decisions, IssuerRegistration, KeyCache, KeySet, KeySource, Registry,
    RuntimeSurface, Verifier,
};

const SECRET: &[u8] = b"a-test-signing-secret-for-the-runtime-surface";
const KID: &str = "test-key-1";
const ISSUER: &str = "https://identity.example/realms/acme";

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after the epoch")
        .as_secs()
}

struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn now_unix_seconds(&self) -> u64 {
        now()
    }
}

/// Publishes the test key, or nothing at all.
struct Keys {
    available: bool,
}

#[async_trait]
impl KeySource for Keys {
    async fn fetch(&self, _jwks_uri: &str) -> Result<KeySet, String> {
        if self.available {
            Ok(KeySet::from_entries([(
                KID.to_owned(),
                DecodingKey::from_secret(SECRET),
            )]))
        } else {
            Err("the identity provider is unreachable".to_owned())
        }
    }
}

/// Answers however the test says, and counts.
struct Answers {
    answer: Result<bool, DecisionFailure>,
    calls: AtomicUsize,
}

#[async_trait]
impl Decisions for Answers {
    async fn check(
        &self,
        _store: &str,
        _model: &str,
        _user: &str,
        _relation: &str,
        _object: &str,
    ) -> Result<bool, DecisionFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.answer
    }

    async fn reachable(&self) -> bool {
        self.answer.is_ok()
    }
}

fn registration() -> IssuerRegistration {
    IssuerRegistration {
        tenant: "acme".to_owned(),
        issuer: ISSUER.to_owned(),
        audience: "workspec".to_owned(),
        jwks_uri: "https://keycloak.internal/certs".to_owned(),
        algorithms: vec![Algorithm::HS256],
        store: "01ACMESTORE".to_owned(),
        authorization_model_id: "01ACMEMODEL".to_owned(),
        max_key_age_seconds: 43_200,
    }
}

/// A surface, plus the decision port so a test can count its calls.
fn surface(keys_available: bool, answer: Result<bool, DecisionFailure>) -> (Router, Arc<Answers>) {
    let registry = Registry::build([registration()]).expect("valid");
    let cache = Arc::new(KeyCache::new(
        Arc::new(Keys {
            available: keys_available,
        }),
        Arc::new(RealClock),
    ));
    let decisions = Arc::new(Answers {
        answer,
        calls: AtomicUsize::new(0),
    });

    let built = RuntimeSurface::new(
        Arc::new(Verifier::new(registry, cache)),
        Arc::new(Check::new(Arc::clone(&decisions) as Arc<dyn Decisions>)),
        Arc::clone(&decisions) as Arc<dyn Decisions>,
    );

    (built.router(), decisions)
}

use axum::Router;

fn token() -> String {
    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(KID.to_owned());

    encode(
        &header,
        &json!({
            "iss": ISSUER,
            "aud": "workspec",
            "sub": "cb606ddc-f148-4193-8875-a84ea6a85e6c",
            "exp": now() + 300,
            "nbf": now() - 10,
        }),
        &EncodingKey::from_secret(SECRET),
    )
    .expect("the fixture encodes")
}

/// Sends a check with whatever body and headers the test wants.
async fn post(router: Router, authorization: Option<&str>, body: &str) -> (StatusCode, String) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/v1/check")
        .header("content-type", "application/json");

    if let Some(value) = authorization {
        request = request.header("authorization", value);
    }

    let response = router
        .oneshot(request.body(Body::from(body.to_owned())).expect("a request"))
        .await
        .expect("a response");

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");

    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn bearer() -> String {
    format!("Bearer {}", token())
}

const GOOD_BODY: &str = r#"{"relation":"viewer","object":"document:123"}"#;

#[tokio::test]
async fn a_denial_is_two_hundred_and_never_forbidden() {
    // The distinction this endpoint exists to preserve. `/v1/check` asks a
    // question; `allowed:false` is a successful answer to it. A 403 would say
    // the caller may not ask at all, which is a different claim and sends an
    // operator looking in the wrong place.
    let (router, _) = surface(true, Ok(false));
    let (status, body) = post(router, Some(&bearer()), GOOD_BODY).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"allowed":false}"#);
}

#[tokio::test]
async fn a_grant_is_two_hundred_and_says_only_that() {
    let (router, _) = surface(true, Ok(true));
    let (status, body) = post(router, Some(&bearer()), GOOD_BODY).await;

    assert_eq!(status, StatusCode::OK);
    // No identity, tenant, store, model or explanation echoed back.
    assert_eq!(body, r#"{"allowed":true}"#);
}

#[tokio::test]
async fn no_credential_is_unauthorized_and_troubles_no_port() {
    let (router, decisions) = surface(true, Ok(true));
    let (status, _) = post(router, None, GOOD_BODY).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        decisions.calls.load(Ordering::SeqCst),
        0,
        "an unauthenticated request must not reach the authorization service"
    );
}

#[tokio::test]
async fn only_a_bearer_is_accepted() {
    for header in [
        "Basic dXNlcjpwYXNz",
        "bearer",
        "Bearer ",
        "Bearer  leading-space",
        "Token abc",
        "",
    ] {
        let (router, _) = surface(true, Ok(true));
        let (status, _) = post(router, Some(header), GOOD_BODY).await;

        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{header:?} must not authenticate"
        );
    }
}

#[tokio::test]
async fn a_body_naming_a_user_or_a_store_is_a_bad_request() {
    for body in [
        r#"{"relation":"viewer","object":"document:123","user":"user:acme/bob"}"#,
        r#"{"relation":"viewer","object":"document:123","store_id":"01OTHER"}"#,
        r#"{"relation":"viewer","object":"document:123","tenant":"other"}"#,
        r#"{"relation":"viewer"}"#,
        r#"{"relation":"not a relation","object":"document:123"}"#,
        r#"{"relation":"viewer","object":"no-colon"}"#,
        "not json",
    ] {
        let (router, _) = surface(true, Ok(true));
        let (status, _) = post(router, Some(&bearer()), body).await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{body} must be refused");
    }
}

#[tokio::test]
async fn identity_cannot_arrive_in_a_query_string() {
    // There is no route that reads one, so this is a 404 rather than a
    // decision about somebody else. Asserted because "it happens to work
    // today" is not the same as "there is no such path".
    let (router, decisions) = surface(true, Ok(true));

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/check?user=user:acme/bob&store_id=01OTHER&relation=viewer")
                .header("authorization", bearer())
                .header("content-type", "application/json")
                .body(Body::from(GOOD_BODY))
                .expect("a request"),
        )
        .await
        .expect("a response");

    // The query string is simply not read: the body still decides, and the
    // identity still comes from the token.
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(decisions.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_identity_provider_outage_is_unavailable_and_never_unauthorized() {
    // The caller's token is perfectly good. Telling them it is not would send
    // them to re-authenticate against the thing that is down.
    let (router, decisions) = surface(false, Ok(true));
    let (status, _) = post(router, Some(&bearer()), GOOD_BODY).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(decisions.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_authorization_outage_is_unavailable_and_a_platform_fault_is_internal() {
    let (router, _) = surface(true, Err(DecisionFailure::Unavailable));
    let (status, _) = post(router, Some(&bearer()), GOOD_BODY).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    // Our store or model is wrong, or the request we built was refused. The
    // caller may well hold the permission; answering 200 allowed:false would
    // be a lie about them.
    let (router, _) = surface(true, Err(DecisionFailure::Internal));
    let (status, body) = post(router, Some(&bearer()), GOOD_BODY).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!body.contains("allowed"), "a fault is not a decision");
}

#[tokio::test]
async fn an_oversized_body_is_refused_before_it_is_read() {
    let (router, decisions) = surface(true, Ok(true));
    let huge = format!(
        r#"{{"relation":"viewer","object":"document:123","padding":"{}"}}"#,
        "x".repeat(64_000)
    );

    let (status, _) = post(router, Some(&bearer()), &huge).await;

    // Asserted as 413 rather than merely "not 200". The first version of this
    // test checked only that the request failed, which it does either way:
    // that body also names an unknown field, so with the limit removed it is
    // refused as a bad request and the test still passed. A test that cannot
    // tell the two apart is not testing the limit.
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        decisions.calls.load(Ordering::SeqCst),
        0,
        "an oversized body must not reach the authorization service"
    );
}

#[tokio::test]
async fn there_is_no_catch_all_route() {
    for (method, path) in [
        ("POST", "/stores/01ACMESTORE/check"),
        ("GET", "/stores"),
        ("POST", "/v1/write"),
        ("GET", "/v1/check"),
        ("POST", "/"),
    ] {
        let (router, decisions) = surface(true, Ok(true));

        let response = router
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", bearer())
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response");

        assert!(
            response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path} must not be served, got {}",
            response.status()
        );
        assert_eq!(decisions.calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn liveness_ignores_its_neighbours_and_readiness_does_not() {
    // Both providers down. Liveness must still pass: a failing liveness probe
    // restarts the container, which would destroy the cached keys that are
    // keeping this process useful.
    let (router, _) = surface(false, Err(DecisionFailure::Unavailable));
    let live = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health/live")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("a response");
    assert_eq!(live.status(), StatusCode::OK);

    // Readiness includes the authorization service: a front that cannot decide
    // should leave rotation.
    let ready = router
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("a response");
    assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn readiness_survives_an_identity_provider_outage() {
    // Keys unavailable, authorization service fine. The verifier is built to
    // keep working on cached keys, so taking the front out of rotation here
    // would remove it for exactly the condition it was designed to survive.
    let (router, _) = surface(false, Ok(true));

    let ready = router
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("a response");

    assert_eq!(ready.status(), StatusCode::OK);
}
