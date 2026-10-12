//! `GET /api/user/current` behind the real OIDC authenticator.
//!
//! Tokens are signed with HS256 against a fixture secret, as
//! `operator::oidc`'s own tests sign them, so no private key exists in this
//! repository; every other decision — issuer, client, role, expiry, signature
//! — is the production authenticator's, reached through the production
//! extractor. The router mounts the two identity routes with the state they
//! read; the full router's wiring is pinned in `tests/user_current.rs`.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::FromRef;
use axum::routing::get;
use axum::Router;
use http::{Request, StatusCode};
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
use serde_json::{json, Value};
use tower::ServiceExt as _;

use super::get_current_user;
use crate::handlers::get_operator;
use crate::operator::{KeyHolder, OidcOperators, OperatorAuthenticator, VerificationKeys};
use crate::operator_tier::OperatorTier;

const SECRET: &[u8] = b"fixture-secret-for-current-user-tests";
const ISSUER: &str = "https://auth.example.test/realms/master";
const CLIENT: &str = "saas-fabric-gateway";
const ROLE: &str = "fabric-operator";

/// An opaque subject that is deliberately not a UUID.
const SUB: &str = "auth0|5f7c8ec7c33c6c004bbafe82";

#[derive(Clone)]
struct IdentityRoutes {
    operators: Arc<dyn OperatorAuthenticator>,
    tier: OperatorTier,
}

impl FromRef<IdentityRoutes> for Arc<dyn OperatorAuthenticator> {
    fn from_ref(state: &IdentityRoutes) -> Self {
        Arc::clone(&state.operators)
    }
}

impl FromRef<IdentityRoutes> for OperatorTier {
    fn from_ref(state: &IdentityRoutes) -> Self {
        state.tier.clone()
    }
}

/// The identity routes behind an authenticator requiring `role`, with the
/// tier taken from that same role, as `build_control_plane` takes it.
fn router(role: &str) -> Router {
    let keys = KeyHolder::empty();
    keys.replace(VerificationKeys::held(vec![(
        None,
        DecodingKey::from_secret(SECRET),
    )]));
    let operators = OidcOperators::new(ISSUER, CLIENT, role, keys, 0)
        .expect("the fixture posture is valid")
        .signed_symmetrically_for_tests();

    Router::new()
        .route("/api/user/current", get(get_current_user))
        .route("/api/operator", get(get_operator))
        .with_state(IdentityRoutes {
            operators: Arc::new(operators),
            tier: OperatorTier::new(role).expect("the fixture role is not blank"),
        })
}

fn now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .as_secs(),
    )
    .expect("the epoch seconds fit in an i64")
}

/// A token every check accepts, with `profile` merged over its claims.
fn token(profile: &Value) -> String {
    signed(&claims(profile), SECRET, Algorithm::HS256)
}

fn claims(profile: &Value) -> Value {
    let mut claims = json!({
        "sub": SUB,
        "iss": ISSUER,
        "azp": CLIENT,
        "exp": now() + 300,
        "realm_access": { "roles": [ROLE] },
    });
    if let (Some(base), Some(extra)) = (claims.as_object_mut(), profile.as_object()) {
        for (name, value) in extra {
            base.insert(name.clone(), value.clone());
        }
    }
    claims
}

fn signed(claims: &Value, secret: &[u8], algorithm: Algorithm) -> String {
    encode(&Header::new(algorithm), claims, &EncodingKey::from_secret(secret)).expect("the fixture signs")
}

/// Sends one request, with every `tracing` event it emits captured.
///
/// Every request in this file goes through here, under a capturing
/// subscriber, for two reasons. Each one checks that the bearer it carried
/// reached neither the body nor the log. And `tracing-core` caches a
/// callsite's interest process-wide from whichever thread first reaches it:
/// one request here with no subscriber could cache `never` for the refusal
/// log and silence it for the test that counts on seeing it (the same hazard
/// `operator::extractor`'s tests document).
async fn ask(router: &Router, path: &str, bearer: Option<&str>) -> (StatusCode, String, String) {
    let log = Captured::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_max_level(tracing::Level::TRACE)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut request = Request::builder().uri(path);
    if let Some(bearer) = bearer {
        request = request.header("authorization", format!("Bearer {bearer}"));
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).expect("a legal request"))
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("a readable body");
    let body = String::from_utf8(bytes.to_vec()).expect("a UTF-8 body");
    let written = String::from_utf8(log.0.lock().expect("an unpoisoned log").clone()).expect("UTF-8");

    if let Some(bearer) = bearer {
        let signature = bearer.rsplit('.').next().unwrap_or(bearer);
        for (place, text) in [("body", &body), ("log", &written)] {
            assert!(!text.contains(bearer), "the bearer reached the {place}");
            assert!(!text.contains(signature), "the signature reached the {place}");
        }
    }

    (status, body, written)
}

async fn current(router: &Router, bearer: &str) -> Value {
    let (status, body, _) = ask(router, "/api/user/current", Some(bearer)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    serde_json::from_str(&body).expect("a JSON body")
}

#[tokio::test]
async fn a_verified_operator_reads_the_whole_profile_with_the_configured_tier_alone() {
    let router = router(ROLE);
    let bearer = token(&json!({
        "preferred_username": "brett",
        "email": "brett@example.test",
        "name": "Brett Example",
        "realm_access": { "roles": [ROLE, "default-roles-master", "offline_access"] },
    }));

    assert_eq!(
        current(&router, &bearer).await,
        json!({
            "subject": SUB,
            "userName": "brett",
            "displayName": "Brett Example",
            "email": "brett@example.test",
            "roles": [ROLE],
        })
    );
}

#[tokio::test]
async fn the_subject_is_the_exact_sub_and_not_the_attribution_name() {
    let router = router(ROLE);
    let bearer = token(&json!({ "preferred_username": "brett" }));

    let (_, operator, _) = ask(&router, "/api/operator", Some(&bearer)).await;
    let operator: Value = serde_json::from_str(&operator).expect("a JSON body");

    assert_eq!(operator, json!({ "subject": "brett" }));
    assert_eq!(current(&router, &bearer).await["subject"], SUB);
}

#[tokio::test]
async fn a_custom_required_role_is_the_only_role_reported() {
    let router = router("platform-admin");
    let bearer = token(&json!({
        "realm_access": { "roles": ["platform-admin", ROLE, "offline_access"] },
    }));

    assert_eq!(
        current(&router, &bearer).await["roles"],
        json!(["platform-admin"])
    );
}

#[tokio::test]
async fn absent_optional_claims_are_null_and_the_display_name_falls_back_to_sub() {
    let body = current(&router(ROLE), &token(&json!({}))).await;

    let object = body.as_object().expect("an object");
    assert_eq!(object.len(), 5, "no field is omitted: {body}");
    assert_eq!(body["userName"], Value::Null);
    assert_eq!(body["email"], Value::Null);
    assert_eq!(body["displayName"], SUB);
}

#[tokio::test]
async fn blank_optional_claims_are_null_and_never_become_the_display_name() {
    let bearer = token(&json!({ "preferred_username": "  ", "email": "", "name": " " }));
    let body = current(&router(ROLE), &bearer).await;

    assert_eq!(body["userName"], Value::Null);
    assert_eq!(body["email"], Value::Null);
    assert_eq!(body["displayName"], SUB);
}

#[tokio::test]
async fn the_display_name_falls_back_to_the_username_when_name_is_absent_or_misshapen() {
    let router = router(ROLE);

    for name in [
        Value::Null,
        json!(42),
        json!({ "given": "Brett" }),
        json!(["Brett"]),
    ] {
        let mut profile = json!({ "preferred_username": "brett", "email": "brett@example.test" });
        if !name.is_null() {
            profile["name"] = name;
        }
        let body = current(&router, &token(&profile)).await;

        assert_eq!(body["displayName"], "brett", "{body}");
        assert_eq!(body["email"], "brett@example.test");
    }
}

#[tokio::test]
async fn no_bearer_is_unauthenticated_and_yields_no_profile() {
    let (status, body, _) = ask(&router(ROLE), "/api/user/current", None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let body: Value = serde_json::from_str(&body).expect("a JSON body");
    assert_eq!(body["error"]["code"], "unauthenticated");
    assert!(body.get("subject").is_none());
}

#[tokio::test]
async fn every_refused_bearer_is_operator_refused_and_yields_no_profile() {
    let router = router(ROLE);
    let profile = json!({ "preferred_username": "brett", "email": "brett@example.test" });
    let mut cases = Vec::new();

    let mut wrong_client = claims(&profile);
    wrong_client["azp"] = json!("another-client");
    cases.push(("wrong client", signed(&wrong_client, SECRET, Algorithm::HS256)));

    let mut realm_member = claims(&profile);
    realm_member["realm_access"] = json!({ "roles": ["default-roles-master", "offline_access"] });
    cases.push((
        "a realm member without the role",
        signed(&realm_member, SECRET, Algorithm::HS256),
    ));

    let mut no_roles = claims(&profile);
    if let Some(object) = no_roles.as_object_mut() {
        object.remove("realm_access");
    }
    cases.push(("no realm roles", signed(&no_roles, SECRET, Algorithm::HS256)));

    let mut wrong_issuer = claims(&profile);
    wrong_issuer["iss"] = json!("https://auth.example.test/realms/acme");
    cases.push(("wrong issuer", signed(&wrong_issuer, SECRET, Algorithm::HS256)));

    let mut expired = claims(&profile);
    expired["exp"] = json!(now() - 600);
    cases.push(("expired", signed(&expired, SECRET, Algorithm::HS256)));

    cases.push((
        "bad signature",
        signed(
            &claims(&profile),
            b"a-key-this-deployment-never-published",
            Algorithm::HS256,
        ),
    ));
    cases.push((
        "an algorithm this deployment does not accept",
        signed(&claims(&profile), SECRET, Algorithm::HS512),
    ));

    for (case, bearer) in cases {
        let (status, body, log) = ask(&router, "/api/user/current", Some(&bearer)).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED, "{case}");
        assert!(log.contains("operator"), "{case}: the refusal is logged: {log}");
        let parsed: Value = serde_json::from_str(&body).expect("a JSON body");
        assert_eq!(parsed["error"]["code"], "operator_refused", "{case}");
        assert!(parsed.get("subject").is_none(), "{case}");
        assert!(!body.contains("brett"), "{case} leaked a claim: {body}");
    }
}

/// A `MakeWriter` collecting everything written, so a test can search the
/// log for a bearer.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("an unpoisoned log").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
