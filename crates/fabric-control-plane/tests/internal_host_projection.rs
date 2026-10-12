//! D01-15: a projected client follows the kind of the client's hosts. A
//! client on `.internal` hosts takes an application as a `privateNetwork`
//! client on those hosts, over the plain HTTP its private network is reached
//! on, and is never given the release's public template callback.
//!
//! The D01-19b semantics hold unchanged around it: every read reports the
//! stored projection, a sweep withholds a diverged or unverifiable entry and
//! leaves identity readiness unmet, and a re-assertion onto one is refused.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::Router;
use fabric_client_model::{ClientId, ClientRevision, OidcClientId, RealmName, RedirectUri};
use fabric_reconciliation::testing::FakeIdentityProvider;
use fabric_reconciliation::{ObservedOidcClient, ReconciliationStatus};
use http::{header, Response, StatusCode};
use serde_json::{json, Value};
use support::projected::{hand_edit_in_git, newco_on_hosts, newco_with_a_projected_client};
use support::{
    as_operator, control_plane, control_plane_with_identity_provider, entity_tag, send, TestControlPlane,
};

const INTERNAL_HOST: &str = "newco.lucentroot.internal";
const INTERNAL_CALLBACK: &str = "http://newco.lucentroot.internal/analytics/callback";
const PUBLIC_CALLBACKS: [&str; 2] = [
    "https://newco.example.com/callback",
    "https://newco.example.com/analytics/callback",
];

fn newco() -> ClientId {
    ClientId::try_new("newco").unwrap()
}

async fn get(router: &Router, path: &str) -> Value {
    let response = send(router, as_operator("GET", path).body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    support::json(response).await
}

/// The projection as the list, the overview and the identity view report
/// it, which must agree.
async fn reported_projection(router: &Router) -> Value {
    let list = get(router, "/api/clients").await;
    let listed = list["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["id"] == "newco")
        .unwrap()["projection"]
        .clone();
    let overview = get(router, "/api/clients/newco").await["projection"].clone();
    let identity = get(router, "/api/clients/newco/identity").await["projection"].clone();
    assert_eq!(listed, overview);
    assert_eq!(overview, identity);
    identity
}

/// The stored redirect block of `newco`'s one projected client.
async fn stored_redirect(router: &Router) -> Value {
    let identity = get(router, "/api/clients/newco/identity").await;
    let clients = identity["clients"].as_array().unwrap();
    assert_eq!(clients.len(), 1, "{identity}");
    assert_eq!(clients[0]["id"], "analytics");
    clients[0]["redirect"].clone()
}

async fn sweep(router: &Router) {
    let response = send(
        router,
        as_operator("POST", "/api/reconciliation")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn settled(plane: &TestControlPlane, revision: &str) {
    let revision = ClientRevision::try_new(revision).unwrap();
    for _ in 0..500 {
        if plane.statuses.report(&newco()).is_some_and(|report| {
            report.revision == revision && report.status != ReconciliationStatus::Pending
        }) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the background convergence never recorded an outcome");
}

async fn converged_on(hosts: &[&str]) -> (TestControlPlane, Arc<FakeIdentityProvider>) {
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_identity_provider(provider.clone());
    let revision = newco_on_hosts(&plane.router, hosts).await;
    settled(&plane, &revision).await;
    (plane, provider)
}

fn analytics_in_provider(provider: &FakeIdentityProvider) -> ObservedOidcClient {
    provider
        .realm(&RealmName::try_new("newco").unwrap())
        .unwrap()
        .clients[&OidcClientId::try_new("analytics").unwrap()]
        .clone()
}

fn callbacks(client: &ObservedOidcClient) -> BTreeSet<&str> {
    client.redirect_uris.iter().map(RedirectUri::as_str).collect()
}

async fn identity_unmet(router: &Router) -> bool {
    let client = get(router, "/api/clients/newco").await;
    client["lifecycle"]["unmet"]
        .as_array()
        .unwrap()
        .iter()
        .any(|unmet| unmet["condition"] == "identity")
}

async fn current_revision(router: &Router) -> String {
    entity_tag(
        &send(
            router,
            as_operator("GET", "/api/clients/newco/identity")
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
}

async fn put_product_on(router: &Router, hosts: &[&str]) -> Response<Body> {
    let revision = current_revision(router).await;
    send(
        router,
        as_operator("PUT", "/api/clients/newco/product")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .body(Body::from(
                json!({
                    "displayName":"Newco","legalName":"Newco Ltd","region":"NZ","timezone":"Pacific/Auckland",
                    "hosts":hosts,"configuration":{},
                    "applications":[{"applicationId":"analytics","version":1,"planId":"standard","configuration":{}}]})
                .to_string(),
            ))
            .unwrap(),
    )
    .await
}

async fn save_product_on(router: &Router, hosts: &[&str]) {
    let response = put_product_on(router, hosts).await;
    let status = response.status();
    assert_eq!(status, StatusCode::OK, "{}", support::json(response).await);
}

async fn refused_code(response: Response<Body>, status: StatusCode) -> Value {
    assert_eq!(response.status(), status);
    support::json(response).await["error"]["code"].clone()
}

#[tokio::test]
async fn an_internal_client_takes_an_application_as_a_private_network_client() {
    let plane = control_plane();
    newco_on_hosts(&plane.router, &[INTERNAL_HOST]).await;

    let redirect = stored_redirect(&plane.router).await;

    assert_eq!(
        redirect,
        json!({"strategy": "privateNetwork", "uris": [INTERNAL_CALLBACK]})
    );
    assert!(
        !redirect.to_string().contains("example.com"),
        "the release's public template is not projected onto an internal client"
    );
    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );
}

#[tokio::test]
async fn a_public_client_is_projected_exactly_as_before() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;

    assert_eq!(
        stored_redirect(&plane.router).await,
        json!({"strategy": "claimedHttps", "uris": PUBLIC_CALLBACKS})
    );
    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );
}

#[tokio::test]
async fn an_internal_client_converges_with_only_its_private_callbacks_and_meets_identity() {
    let (plane, provider) = converged_on(&[INTERNAL_HOST]).await;

    let analytics = analytics_in_provider(&provider);
    assert_eq!(callbacks(&analytics), BTreeSet::from([INTERNAL_CALLBACK]));
    assert!(analytics.public);
    let identity = get(&plane.router, "/api/clients/newco/identity").await;
    assert_eq!(identity["reconciliation"]["status"], "applied", "{identity}");
    assert_eq!(identity["reconciliation"]["withheld"], json!([]));
    assert!(!identity_unmet(&plane.router).await);
}

#[tokio::test]
async fn a_product_save_moving_hosts_between_kinds_reprojects_and_stays_consistent() {
    let plane = control_plane();
    newco_with_a_projected_client(&plane.router).await;

    save_product_on(&plane.router, &[INTERNAL_HOST]).await;
    assert_eq!(
        stored_redirect(&plane.router).await,
        json!({"strategy": "privateNetwork", "uris": [INTERNAL_CALLBACK]})
    );
    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );

    save_product_on(&plane.router, &["newco.example.com"]).await;
    assert_eq!(
        stored_redirect(&plane.router).await,
        json!({"strategy": "claimedHttps", "uris": PUBLIC_CALLBACKS})
    );
    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );
}

#[tokio::test]
async fn a_git_edit_to_an_internal_projection_is_reported_withheld_and_refuses_reassertion() {
    let (plane, provider) = converged_on(&[INTERNAL_HOST]).await;
    let before = analytics_in_provider(&provider);
    let revision = hand_edit_in_git(&plane, |text| {
        text.replace(
            INTERNAL_CALLBACK,
            "https://newco.lucentroot.internal/analytics/callback",
        )
    })
    .await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "diverged", "divergences": [{"client": "analytics", "field": "redirectUris"}]})
    );
    sweep(&plane.router).await;
    assert_eq!(
        analytics_in_provider(&provider),
        before,
        "the Git edit is not converged"
    );
    let identity = get(&plane.router, "/api/clients/newco/identity").await;
    assert_eq!(identity["reconciliation"]["status"], "withheld", "{identity}");
    assert_eq!(identity["reconciliation"]["withheld"], json!(["analytics"]));
    assert!(identity_unmet(&plane.router).await);

    let response = send(
        &plane.router,
        as_operator("POST", "/api/clients/newco/identity/reassertion")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        support::json(response).await["error"]["code"],
        "projected_client_diverged"
    );
}

#[tokio::test]
async fn a_git_edit_moving_a_public_client_onto_internal_hosts_alone_diverges_and_is_withheld() {
    let (plane, provider) = converged_on(&["newco.example.com"]).await;
    let before = analytics_in_provider(&provider);
    hand_edit_in_git(&plane, |text| {
        text.replace("  - newco.example.com\n", &format!("  - {INTERNAL_HOST}\n"))
    })
    .await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "diverged", "divergences": [{"client": "analytics", "field": "redirectStrategy"}]})
    );
    sweep(&plane.router).await;
    assert_eq!(
        analytics_in_provider(&provider),
        before,
        "the public callbacks stay as converged until the projection is restored"
    );
    let identity = get(&plane.router, "/api/clients/newco/identity").await;
    assert_eq!(identity["reconciliation"]["status"], "withheld", "{identity}");
    assert!(identity_unmet(&plane.router).await);
    assert_eq!(
        refused_code(
            put_product_on(&plane.router, &[INTERNAL_HOST]).await,
            StatusCode::CONFLICT
        )
        .await,
        "projected_client_diverged",
        "a product save does not silently replace the diverged entry"
    );

    let mut restored =
        json!({"realm": identity["realm"], "roles": identity["roles"], "clients": identity["clients"]});
    restored["clients"][0]["redirect"] = json!({"strategy": "privateNetwork", "uris": [INTERNAL_CALLBACK]});
    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/newco/identity")
            .header(
                header::IF_MATCH,
                format!("\"{}\"", current_revision(&plane.router).await),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(restored.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "an identity edit restoring the projection is the repair"
    );
    settled(&plane, &entity_tag(&response)).await;
    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "consistent"})
    );
    let identity = get(&plane.router, "/api/clients/newco/identity").await;
    assert_eq!(
        identity["reconciliation"]["handEdits"],
        json!([{"client": "analytics", "field": "declaredRedirectUriRemoved"}]),
        "as for any converged client whose hosts change, D01-19b holds the new callback as a removed one: {identity}"
    );

    let response = send(
        &plane.router,
        as_operator("POST", "/api/clients/newco/identity/reassertion")
            .header(
                header::IF_MATCH,
                format!("\"{}\"", current_revision(&plane.router).await),
            )
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        callbacks(&analytics_in_provider(&provider)),
        BTreeSet::from([INTERNAL_CALLBACK]),
        "the audited re-assertion writes the private callback, and no public one is left"
    );
}

#[tokio::test]
async fn a_git_edit_mixing_host_kinds_is_unverifiable_and_withheld() {
    let (plane, provider) = converged_on(&["newco.example.com"]).await;
    let before = analytics_in_provider(&provider);
    hand_edit_in_git(&plane, |text| {
        text.replace(
            "  - newco.example.com\n",
            &format!("  - newco.example.com\n  - {INTERNAL_HOST}\n"),
        )
    })
    .await;

    assert_eq!(
        reported_projection(&plane.router).await,
        json!({"state": "unverifiable", "unverified": ["analytics"]})
    );
    sweep(&plane.router).await;
    assert_eq!(analytics_in_provider(&provider), before);
    let identity = get(&plane.router, "/api/clients/newco/identity").await;
    assert_eq!(identity["reconciliation"]["status"], "withheld", "{identity}");
    assert!(identity_unmet(&plane.router).await);
    assert_eq!(
        refused_code(
            put_product_on(&plane.router, &["newco.example.com"]).await,
            StatusCode::INTERNAL_SERVER_ERROR
        )
        .await,
        "desired_state_invalid",
        "an unverifiable stored projection is repaired in Git, as before"
    );
}
