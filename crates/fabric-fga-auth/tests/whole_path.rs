//! The assembled system: real Keycloak, real OpenFGA, the real HTTP surface.
//!
//! # Activation is explicit, and enabling it removes every escape
//!
//! ```text
//! (nothing set)  → SKIPPED, and says so
//! FABRIC_E2E=1   → missing configuration is a FAILURE, not a skip
//!                  setup failure is a FAILURE
//!                  test failure is a FAILURE
//! ```
//!
//! We have been bitten repeatedly by tests that pass without proving
//! anything. Once somebody has asked for this proof, it must be impossible to
//! get green by quietly not running it.
//!
//! # What this proves that no component test can
//!
//! **The issuer a token names is not the address the keys come from.** The
//! fixture points Keycloak's `KC_HOSTNAME` at an issuer nothing can reach, and
//! the registry's `jwks_uri` at the address this process actually has. If that
//! passes, Fabric can serve a public issuer from inside a cluster — the thing
//! Topaz's request-forgery filter made impossible.
//!
//! **Two realms, through one front and one OpenFGA.** A single-realm run would
//! prove JWT verification and OpenFGA integration and still not prove the
//! reason this component exists.
//!
//! # Running it
//!
//! ```text
//! ./scripts/e2e-services.sh up      # Keycloak and OpenFGA on fixed ports
//! FABRIC_E2E=1 cargo test -p fabric-fga-auth --test whole_path
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines
)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use fabric_core::SystemClock;
use fabric_fga_auth::{
    Check, HttpKeySource, IssuerRegistration, KeyCache, KeySet, KeySource, OpenFgaDecisions, Registry,
    RuntimeSurface, Verifier,
};
use jsonwebtoken::Algorithm;
use tower::ServiceExt as _;

/// Where the fixture's services are, once activation has insisted on them.
struct Services {
    /// The address this process reaches Keycloak on.
    keycloak: String,

    /// The issuer tokens carry, which is deliberately not reachable.
    issuer_base: String,

    /// The port the loopback OpenFGA listens on.
    openfga: u16,
}

/// Reads the activation contract.
///
/// Returns `None` when the suite is off. When it is **on**, anything missing
/// is a panic rather than a skip: an enabled proof that quietly does nothing
/// is worse than no proof, because it reports success.
fn services() -> Option<Services> {
    if std::env::var("FABRIC_E2E").ok().as_deref() != Some("1") {
        eprintln!("SKIPPED: FABRIC_E2E is not enabled (see this file's documentation)");
        return None;
    }

    let required = |name: &str| -> String {
        std::env::var(name).unwrap_or_else(|_| {
            panic!("FABRIC_E2E=1 but {name} is not set; an enabled end-to-end suite must not skip")
        })
    };

    Some(Services {
        keycloak: required("FABRIC_E2E_KEYCLOAK"),
        issuer_base: required("FABRIC_E2E_ISSUER_BASE"),
        openfga: required("FABRIC_E2E_OPENFGA_PORT")
            .parse()
            .expect("FABRIC_E2E_OPENFGA_PORT must be a port number"),
    })
}

/// A key source that can be switched off, wrapping the real one.
///
/// The provider stays real; only the network to it is taken away, which is the
/// condition the cache's rules exist for.
struct Interruptible {
    inner: HttpKeySource,
    reachable: AtomicBool,
}

#[async_trait]
impl KeySource for Interruptible {
    async fn fetch(&self, jwks_uri: &str) -> Result<KeySet, String> {
        if self.reachable.load(Ordering::SeqCst) {
            self.inner.fetch(jwks_uri).await
        } else {
            Err("the identity provider is unreachable".to_owned())
        }
    }
}

/// A user's access token from one realm, and its subject.
///
/// The token endpoint comes from the realm's **discovery document** rather
/// than from a path this file knows. That is protocol-correct — and it is also
/// what keeps a provider's own URL layout out of a crate that must not know it
/// (ADR 0008). The realms themselves are created by `scripts/e2e-services.sh`
/// for the same reason.
async fn user_token(services: &Services, realm: &str) -> (String, String) {
    let http = reqwest::Client::new();

    let discovery: serde_json::Value = http
        .get(format!(
            "{}/realms/{realm}/.well-known/openid-configuration",
            services.keycloak
        ))
        .send()
        .await
        .expect("the identity provider must be reachable for an enabled end-to-end run")
        .json()
        .await
        .expect("a discovery document");

    // Everything discovery advertises carries the *public* issuer, which the
    // fixture has made unreachable on purpose. Rewriting the base is what a
    // caller inside the network does — and what a deployment does when it
    // configures an internal address for a logical endpoint.
    let advertised = discovery["token_endpoint"]
        .as_str()
        .unwrap_or_else(|| panic!("no token endpoint for realm {realm}; is the fixture up?"));
    let endpoint = advertised.replacen(&services.issuer_base, &services.keycloak, 1);

    let response: serde_json::Value = http
        .post(&endpoint)
        .form(&[
            ("grant_type", "password"),
            ("client_id", "app"),
            ("username", "tenantuser"),
            ("password", "e2e-fixture-password"),
            ("scope", "openid"),
        ])
        .send()
        .await
        .expect("a token response")
        .json()
        .await
        .expect("a token body");

    let token = response["access_token"]
        .as_str()
        .unwrap_or_else(|| panic!("no access token for realm {realm}: run scripts/e2e-services.sh up"))
        .to_owned();

    let claims = token.split('.').nth(1).expect("a payload");
    let padded = format!("{claims}{}", "=".repeat((4 - claims.len() % 4) % 4));
    let parsed: serde_json::Value = serde_json::from_slice(&base64_decode(&padded)).expect("claims");

    let issuer = parsed["iss"].as_str().expect("an issuer").to_owned();
    assert!(
        issuer.starts_with(&services.issuer_base),
        "the fixture must mint tokens for the unreachable issuer, got {issuer}"
    );

    (token, parsed["sub"].as_str().expect("a subject").to_owned())
}

/// Minimal URL-safe base64, so the test needs no extra dependency.
fn base64_decode(value: &str) -> Vec<u8> {
    use base64::Engine as _;

    base64::engine::general_purpose::URL_SAFE
        .decode(value)
        .expect("a base64 payload")
}

/// Creates a store and model in OpenFGA, granting `viewer` to `subject`.
async fn ensure_store(services: &Services, subject: &str) -> (String, String) {
    let http = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{}", services.openfga);

    let store: serde_json::Value = http
        .post(format!("{base}/stores"))
        .json(&serde_json::json!({ "name": "fabric-e2e" }))
        .send()
        .await
        .expect("OpenFGA must be reachable for an enabled end-to-end run")
        .json()
        .await
        .expect("a store");
    let store_id = store["id"].as_str().expect("a store id").to_owned();

    let model: serde_json::Value = http
        .post(format!("{base}/stores/{store_id}/authorization-models"))
        .json(&serde_json::json!({
            "schema_version": "1.1",
            "type_definitions": [
                { "type": "user" },
                {
                    "type": "document",
                    "relations": { "viewer": { "this": {} } },
                    "metadata": { "relations": {
                        "viewer": { "directly_related_user_types": [{ "type": "user" }] }
                    }}
                }
            ]
        }))
        .send()
        .await
        .expect("create model")
        .json()
        .await
        .expect("a model");
    let model_id = model["authorization_model_id"]
        .as_str()
        .expect("a model id")
        .to_owned();

    http.post(format!("{base}/stores/{store_id}/write"))
        .json(&serde_json::json!({
            "authorization_model_id": model_id,
            "writes": { "tuple_keys": [
                { "user": subject, "relation": "viewer", "object": "document:granted" }
            ]}
        }))
        .send()
        .await
        .expect("write tuple");

    (store_id, model_id)
}

/// Where this process can actually read a realm's keys.
///
/// Discovery is fetched from the address we *have*, and reports the address
/// the provider *advertises* — which the fixture has deliberately made
/// unreachable. Rewriting the base is exactly what a deployment does when it
/// configures `jwks_uri`: the logical endpoint is the provider's, the address
/// is the operator's. Doing it this way also keeps the provider's URL layout
/// out of this file (ADR 0008).
async fn internal_jwks_uri(services: &Services, realm: &str) -> String {
    let discovery: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{}/realms/{realm}/.well-known/openid-configuration",
            services.keycloak
        ))
        .send()
        .await
        .expect("the identity provider must be reachable")
        .json()
        .await
        .expect("a discovery document");

    let advertised = discovery["jwks_uri"].as_str().expect("a jwks_uri");

    assert!(
        advertised.starts_with(&services.issuer_base),
        "the fixture must advertise the unreachable issuer, got {advertised}"
    );

    advertised.replacen(&services.issuer_base, &services.keycloak, 1)
}

/// One tenant's registration, wired so the issuer and the key address differ.
fn registration(
    services: &Services,
    realm: &str,
    jwks_uri: String,
    store: &str,
    model: &str,
) -> IssuerRegistration {
    IssuerRegistration {
        tenant: realm.to_owned(),

        // What the token says, and what nothing can resolve.
        issuer: format!("{}/realms/{realm}", services.issuer_base),

        audience: "openfga".to_owned(),

        // Where this process can actually read keys. The whole point.
        jwks_uri,

        algorithms: vec![Algorithm::RS256],
        store: store.to_owned(),
        authorization_model_id: model.to_owned(),
        max_key_age_seconds: 43_200,
    }
}

/// Everything assembled, plus the switch that takes the key network away.
struct WholePath {
    router: axum::Router,
    keys: Arc<Interruptible>,
}

impl WholePath {
    fn over(services: &Services, registrations: Vec<IssuerRegistration>, openfga_port: u16) -> Self {
        let keys = Arc::new(Interruptible {
            inner: HttpKeySource::new().expect("a key source"),
            reachable: AtomicBool::new(true),
        });

        let verifier = Verifier::new(
            Registry::build(registrations).expect("a valid registry"),
            Arc::new(KeyCache::new(
                Arc::clone(&keys) as Arc<dyn KeySource>,
                Arc::new(SystemClock),
            )),
        );

        let decisions = Arc::new(OpenFgaDecisions::on_loopback(openfga_port).expect("a client"));
        let surface = RuntimeSurface::new(
            Arc::new(verifier),
            Arc::new(Check::new(
                Arc::clone(&decisions) as Arc<dyn fabric_fga_auth::Decisions>
            )),
            decisions as Arc<dyn fabric_fga_auth::Decisions>,
        );

        let _ = services;

        Self {
            router: surface.router(),
            keys,
        }
    }

    /// Asks the surface, exactly as a client would.
    async fn check(&self, token: &str, relation: &str, object: &str) -> (StatusCode, String) {
        let body = serde_json::json!({ "relation": relation, "object": object }).to_string();

        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/check")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .expect("a request"),
            )
            .await
            .expect("a response");

        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("a body");

        (status, String::from_utf8_lossy(&bytes).to_string())
    }
}

#[tokio::test]
async fn the_whole_runtime_path_holds_for_two_independently_trusted_realms() {
    let Some(services) = services() else {
        return;
    };

    let (acme_token, acme_sub) = user_token(&services, "tworealmsacme").await;
    let (foo_token, foo_sub) = user_token(&services, "tworealmsfoo").await;

    // Realm-qualified exactly as `SubjectId` renders, which is what the
    // front will send.
    let (acme_store, acme_model) = ensure_store(&services, &format!("user:tworealmsacme/{acme_sub}")).await;
    let (foo_store, foo_model) = ensure_store(&services, &format!("user:tworealmsfoo/{foo_sub}")).await;

    let path = WholePath::over(
        &services,
        vec![
            registration(
                &services,
                "tworealmsacme",
                internal_jwks_uri(&services, "tworealmsacme").await,
                &acme_store,
                &acme_model,
            ),
            registration(
                &services,
                "tworealmsfoo",
                internal_jwks_uri(&services, "tworealmsfoo").await,
                &foo_store,
                &foo_model,
            ),
        ],
        services.openfga,
    );

    // A public issuer nothing can resolve, keys read from an address only this
    // process has, and a real decision at the end of it.
    let (status, body) = path.check(&acme_token, "viewer", "document:granted").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"allowed":true}"#, "the granted tuple must be found");

    let (status, body) = path.check(&acme_token, "viewer", "document:ungranted").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body, r#"{"allowed":false}"#,
        "an absent tuple is a denial, not an error"
    );

    // The reason this component exists: a second realm, independently trusted,
    // through the same front and the same authorization service.
    let (status, body) = path.check(&foo_token, "viewer", "document:granted").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"allowed":true}"#, "the second realm must work too");

    // And its store is its own: foo's token cannot reach acme's grant, because
    // the store came from foo's registration and the principal carries foo's
    // realm.
    let (status, body) = path.check(&foo_token, "viewer", "document:ungranted").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"allowed":false}"#);
}

#[tokio::test]
async fn a_token_from_an_unregistered_realm_is_refused() {
    let Some(services) = services() else {
        return;
    };

    let (acme_token, acme_sub) = user_token(&services, "unregacme").await;
    let (rogue_token, _) = user_token(&services, "unregrogue").await;
    let (store, model) = ensure_store(&services, &format!("user:tworealmsacme/{acme_sub}")).await;

    // Only acme is registered.
    let path = WholePath::over(
        &services,
        vec![registration(
            &services,
            "unregacme",
            internal_jwks_uri(&services, "unregacme").await,
            &store,
            &model,
        )],
        services.openfga,
    );

    let (status, _) = path.check(&rogue_token, "viewer", "document:granted").await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a real, correctly signed token from an unregistered realm must be refused"
    );

    // The registered realm still works, so the refusal was about trust rather
    // than the fixture being broken.
    let (status, _) = path.check(&acme_token, "viewer", "document:granted").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_real_token_cannot_name_its_own_store_tenant_or_principal() {
    let Some(services) = services() else {
        return;
    };

    let (token, sub) = user_token(&services, "injectacme").await;
    let (_, victim_sub) = user_token(&services, "injectvictim").await;

    let (acme_store, acme_model) = ensure_store(&services, &format!("user:injectacme/{sub}")).await;
    // A store the caller must not be able to reach, with a grant in it.
    let (victim_store, _) = ensure_store(&services, &format!("user:injectvictim/{victim_sub}")).await;

    let path = WholePath::over(
        &services,
        vec![registration(
            &services,
            "injectacme",
            internal_jwks_uri(&services, "injectacme").await,
            &acme_store,
            &acme_model,
        )],
        services.openfga,
    );

    // Every field a caller might hope influences routing. None is in the
    // schema, so each is a bad request rather than a decision about somebody
    // else's store.
    for extra in [
        serde_json::json!({ "user": format!("user:injectvictim/{victim_sub}") }),
        serde_json::json!({ "store": victim_store.clone() }),
        serde_json::json!({ "store_id": victim_store.clone() }),
        serde_json::json!({ "authorization_model_id": "01ANYTHING" }),
        serde_json::json!({ "tenant": "injectvictim" }),
        serde_json::json!({ "principal": format!("injectvictim/{victim_sub}") }),
    ] {
        let mut body = serde_json::json!({ "relation": "viewer", "object": "document:granted" });
        for (key, value) in extra.as_object().expect("an object") {
            body[key] = value.clone();
        }

        let response = path
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/check")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("a request"),
            )
            .await
            .expect("a response");

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "a body carrying {extra} must be refused, not answered"
        );
    }

    // The ordinary request still works, so the refusals were about the extra
    // fields rather than the fixture being broken.
    let (status, body) = path.check(&token, "viewer", "document:granted").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"allowed":true}"#);
}

#[tokio::test]
async fn the_audience_is_enforced_against_a_real_provider() {
    let Some(services) = services() else {
        return;
    };

    let (token, sub) = user_token(&services, "audienceacme").await;
    let (store, model) = ensure_store(&services, &format!("user:audienceacme/{sub}")).await;

    let mut wrong = registration(
        &services,
        "audienceacme",
        internal_jwks_uri(&services, "audienceacme").await,
        &store,
        &model,
    );
    wrong.audience = "something-this-token-does-not-carry".to_owned();

    let path = WholePath::over(&services, vec![wrong], services.openfga);
    let (status, _) = path.check(&token, "viewer", "document:granted").await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_unreachable_authorization_service_is_unavailable_not_a_denial() {
    let Some(services) = services() else {
        return;
    };

    let (token, sub) = user_token(&services, "outageacme").await;
    let (store, model) = ensure_store(&services, &format!("user:outageacme/{sub}")).await;

    // A port with nothing on it.
    let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = dead.local_addr().unwrap().port();
    drop(dead);

    let path = WholePath::over(
        &services,
        vec![registration(
            &services,
            "outageacme",
            internal_jwks_uri(&services, "outageacme").await,
            &store,
            &model,
        )],
        port,
    );

    let (status, body) = path.check(&token, "viewer", "document:granted").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(!body.contains("allowed"), "an outage is not a decision");
}

#[tokio::test]
async fn an_identity_provider_outage_does_not_stop_a_known_key_working() {
    let Some(services) = services() else {
        return;
    };

    let (token, sub) = user_token(&services, "cachedacme").await;
    let (store, model) = ensure_store(&services, &format!("user:cachedacme/{sub}")).await;

    let path = WholePath::over(
        &services,
        vec![registration(
            &services,
            "cachedacme",
            internal_jwks_uri(&services, "cachedacme").await,
            &store,
            &model,
        )],
        services.openfga,
    );

    // Warm the cache against the real provider.
    let (status, _) = path.check(&token, "viewer", "document:granted").await;
    assert_eq!(status, StatusCode::OK);

    // Now take the provider away entirely.
    path.keys.reachable.store(false, Ordering::SeqCst);

    let (status, body) = path.check(&token, "viewer", "document:granted").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a cached key must keep working through a provider outage"
    );
    assert_eq!(body, r#"{"allowed":true}"#);

    // An unfamiliar `kid`, with the provider down. The answer follows the
    // verifier's evidence rather than the token: a key is refused only when a
    // fresh successful snapshot positively says the issuer does not publish
    // it, and otherwise the honest answer is that we cannot tell.
    let unknown_kid = token_with_unknown_kid(&token);

    // The snapshot fetched moments ago still proves absence, so refusing is a
    // statement of what the issuer publishes rather than a guess.
    let (status, _) = path.check(&unknown_kid, "viewer", "document:granted").await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a fresh successful snapshot is evidence, and absence from it is a refusal"
    );

    // Once that evidence has aged out and the provider cannot renew it, the
    // same token gets a different answer — correctly, because we no longer
    // know enough to make the claim we made a moment ago.
    tokio::time::sleep(std::time::Duration::from_secs(
        fabric_fga_auth::UNKNOWN_KID_FRESHNESS_SECONDS + 1,
    ))
    .await;

    let (status, _) = path.check(&unknown_kid, "viewer", "document:granted").await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "without fresh evidence a missing key is unavailable, never unauthorized"
    );

    // And the known key still works throughout, which is the property that
    // matters most: an identity-provider outage must not take the front down.
    let (status, body) = path.check(&token, "viewer", "document:granted").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"allowed":true}"#);
}

/// The same token with its `kid` changed, so the cache cannot serve it.
///
/// The signature no longer matches the header, which is fine: the point is
/// that verification cannot even *reach* a signature check without a key, and
/// the failure to obtain one is what is under test.
fn token_with_unknown_kid(token: &str) -> String {
    use base64::Engine as _;

    let parts: Vec<&str> = token.split('.').collect();
    let header = serde_json::json!({ "alg": "RS256", "typ": "JWT", "kid": "never-published" });
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(header.to_string());

    format!("{encoded}.{}.{}", parts[1], parts[2])
}
