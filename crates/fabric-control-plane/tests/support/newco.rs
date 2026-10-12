//! Reading and driving `newco`, the client `projected` builds, through the
//! real router: what every read reports, what the fake provider holds, and
//! the product saves and passes a host transition is made of (D01-15).

// Reading a JSON field is the assertion here; see `mod.rs`.
#![allow(clippy::indexing_slicing)]

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

use super::projected::newco_on_hosts;
use super::{
    as_operator, control_plane_with_identity_provider, entity_tag, json as body_json, send, TestControlPlane,
};

/// `newco`'s one `.internal` host in these tests.
pub const INTERNAL_HOST: &str = "newco.lucentroot.internal";

/// The callback `analytics` projects on [`INTERNAL_HOST`] over `scheme`.
pub fn internal_callback(scheme: &str) -> String {
    format!("{scheme}://{INTERNAL_HOST}/analytics/callback")
}

/// What `analytics` projects for `newco` on `newco.example.com`.
pub const PUBLIC_CALLBACKS: [&str; 2] = [
    "https://newco.example.com/callback",
    "https://newco.example.com/analytics/callback",
];

/// `newco`'s id.
pub fn newco() -> ClientId {
    ClientId::try_new("newco").unwrap()
}

/// A `GET` that must succeed, as JSON.
pub async fn get(router: &Router, path: &str) -> Value {
    let response = send(router, as_operator("GET", path).body(Body::empty()).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    body_json(response).await
}

/// The projection as the list, the overview and the identity view report
/// it, which must agree.
pub async fn reported_projection(router: &Router) -> Value {
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
pub async fn stored_redirect(router: &Router) -> Value {
    let identity = get(router, "/api/clients/newco/identity").await;
    let clients = identity["clients"].as_array().unwrap();
    assert_eq!(clients.len(), 1, "{identity}");
    assert_eq!(clients[0]["id"], "analytics");
    clients[0]["redirect"].clone()
}

/// `newco`'s reconciliation as the identity view reports it.
pub async fn reconciliation(router: &Router) -> Value {
    get(router, "/api/clients/newco/identity").await["reconciliation"].clone()
}

/// One sweep, through the operator's door.
pub async fn sweep(router: &Router) {
    let response = send(
        router,
        as_operator("POST", "/api/reconciliation")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// Waits for the convergence a write starts in the background to record
/// its outcome for `revision`, so it cannot land after a later step.
pub async fn settled(plane: &TestControlPlane, revision: &str) {
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

/// `newco` created on `hosts` and converged into a fresh fake provider.
pub async fn converged_on(
    hosts: &[&str],
    scheme: Option<&str>,
) -> (TestControlPlane, Arc<FakeIdentityProvider>) {
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_identity_provider(provider.clone());
    let revision = newco_on_hosts(&plane.router, hosts, scheme).await;
    settled(&plane, &revision).await;
    (plane, provider)
}

/// What the fake provider holds for `analytics`.
pub fn analytics_in_provider(provider: &FakeIdentityProvider) -> ObservedOidcClient {
    provider
        .realm(&RealmName::try_new("newco").unwrap())
        .unwrap()
        .clients[&OidcClientId::try_new("analytics").unwrap()]
        .clone()
}

/// The callbacks the fake provider holds for `analytics`.
pub fn provider_callbacks(provider: &FakeIdentityProvider) -> BTreeSet<String> {
    analytics_in_provider(provider)
        .redirect_uris
        .iter()
        .map(RedirectUri::to_string)
        .collect()
}

/// Whether `newco`'s lifecycle reports identity unmet.
pub async fn identity_unmet(router: &Router) -> bool {
    let client = get(router, "/api/clients/newco").await;
    client["lifecycle"]["unmet"]
        .as_array()
        .unwrap()
        .iter()
        .any(|unmet| unmet["condition"] == "identity")
}

/// `newco`'s current revision.
pub async fn current_revision(router: &Router) -> String {
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

/// A product save moving `newco` onto `hosts`, keeping `analytics`.
pub async fn put_product_on(router: &Router, hosts: &[&str], scheme: Option<&str>) -> Response<Body> {
    let revision = current_revision(router).await;
    send(
        router,
        as_operator("PUT", "/api/clients/newco/product")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .body(Body::from(
                json!({
                    "displayName":"Newco","legalName":"Newco Ltd","region":"NZ","timezone":"Pacific/Auckland",
                    "hosts":hosts,"privateNetworkScheme":scheme,"configuration":{},
                    "applications":[{"applicationId":"analytics","version":1,"planId":"standard","configuration":{}}]})
                .to_string(),
            ))
            .unwrap(),
    )
    .await
}

/// As [`put_product_on`], which must succeed; returns the new revision.
pub async fn save_product_on(router: &Router, hosts: &[&str], scheme: Option<&str>) -> String {
    let response = put_product_on(router, hosts, scheme).await;
    let Some(tag) = (response.status() == StatusCode::OK).then(|| entity_tag(&response)) else {
        panic!("the save was refused: {}", body_json(response).await);
    };
    tag
}

/// The status and error body of a refused request.
pub async fn refusal(response: Response<Body>) -> (StatusCode, Value) {
    let status = response.status();
    (status, body_json(response).await["error"].clone())
}

/// An operator's re-assertion of `newco` at its current revision.
pub async fn reassert(router: &Router) -> Response<Body> {
    let revision = current_revision(router).await;
    send(
        router,
        as_operator("POST", "/api/clients/newco/identity/reassertion")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .body(Body::empty())
            .unwrap(),
    )
    .await
}
