//! Product workflows exercise authentication, immutable releases and real client documents.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support;
use std::sync::Arc;

use axum::{body::Body, Router};
use fabric_client_model::catalogue::{Catalogue, ClientProduct, ClientProductRequest, ProductActivity};
use fabric_client_model::{ClientDocument, ClientId, Host};
use fabric_control_plane::{ChangeContext, ClientRepository};
use fabric_reconciliation::testing::FakeIdentityProvider;
use http::{header, Response, StatusCode};
use serde_json::{json, Value};
use support::{
    as_operator, control_plane, control_plane_with_identity_provider, entity_tag, send, OPERATOR,
    OPERATOR_REALM,
};

async fn command(router: &Router, revision: &Value, value: Value) -> Value {
    let builder = as_operator("POST", "/api/catalogue").header("content-type", "application/json");
    let builder = if let Some(revision) = revision.as_str() {
        builder.header("if-match", format!("\"{revision}\""))
    } else {
        builder.header("if-none-match", "*")
    };
    let response = send(router, builder.body(Body::from(value.to_string())).unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
    support::json(response).await
}
fn definition() -> Value {
    json!({"name":"Analytics", "description":"Reports", "domain":"{client}.example.com",
        "components":[{"id":"reports", "name":"Reports", "kind":"container", "reference":"registry.example.com/reports", "version":"1.0.0", "required":false,"policy":"manual"}],
        "features":[{"id":"reporting","name":"Reporting","description":"", "implementedBy":["reports"]}],
        "plans":[{"id":"standard","name":"Standard","description":"","features":["reporting"],"configuration":{"seats":"10"}}],
        "fields":[{"key":"team","label":"Team","kind":"text","required":true,"default":null,"options":[],"description":""}],
        "navigation":[{"label":"Reports","route":"/reports","feature":"reporting","permission":"reports.read"}]})
}
fn client() -> Value {
    json!({"displayName":"Acme", "legalName":"Acme Ltd", "region":"NZ", "timezone":"Pacific/Auckland", "hosts":["newco.example.com"], "configuration":{},
        "applications":[{"applicationId":"analytics","version":1,"planId":"standard","configuration":{"team":"Finance"}}]})
}
// `too_many_lines` is allowed here for the same reason
// `control_plane_api.rs`'s `a_native_client_is_declared_reconciled_...` test
// allows it: this is one composed workflow against the real router —
// publish a release, create a client against it, edit the release and show
// the client kept the old one, then show a second application whose id
// collides with a hand-declared identity client is refused on its very
// first assignment. Splitting the collision check into its own test would
// mean rebuilding this same published catalogue and client from scratch to
// reach the state it depends on; kept here, it reuses what the test already
// built.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn published_assignment_is_immutable_and_creates_identity() {
    let app = control_plane();
    let first = command(
        &app.router,
        &Value::Null,
        json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
    )
    .await;
    let saved = command(
        &app.router,
        &first["revision"],
        json!({"action":"saveApplication","id":"analytics","definition":definition()}),
    )
    .await;
    let published = command(
        &app.router,
        &saved["revision"],
        json!({"action":"publishApplication","id":"analytics","note":"Initial release"}),
    )
    .await;
    let mut changed = definition();
    changed["name"] = json!("Next draft");
    let edited = command(
        &app.router,
        &published["revision"],
        json!({"action":"saveApplication","id":"analytics","definition":changed}),
    )
    .await;
    assert_eq!(
        edited["catalogue"]["applications"][0]["releases"][0]["definition"]["name"],
        "Analytics"
    );
    let response = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":client()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let created = support::json(response).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["resolved"][0]["components"][0]["id"], "reports");
    assert_eq!(created["resolved"][0]["navigation"][0]["route"], "/reports");
    assert_eq!(
        created["product"]["applications"][0]["release"]["definition"]["name"],
        "Analytics"
    );
    let identity = send(
        &app.router,
        as_operator("GET", "/api/clients/newco/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let identity = support::json(identity).await;
    assert_eq!(identity["clients"][0]["id"], "analytics");
    // The exact callback set `with_application_identity` builds: the
    // template callback from the application's own domain, and one more per
    // declared host — no fewer, and nothing extra either.
    let uris = identity["clients"][0]["redirect"]["uris"]
        .as_array()
        .unwrap()
        .iter()
        .map(|uri| uri.as_str().unwrap().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        uris,
        std::collections::BTreeSet::from([
            "https://newco.example.com/callback".to_owned(),
            "https://newco.example.com/analytics/callback".to_owned(),
        ])
    );

    // A hand-declared identity client sharing an application's id must
    // refuse the *first* assignment of that application — not only a
    // change to an existing one. `acme`'s fixture identity already
    // declares an OIDC client named `web`.
    let web_created = command(
        &app.router,
        &edited["revision"],
        json!({"action":"createApplication","id":"web","name":"Web"}),
    )
    .await;
    let web_saved = command(
        &app.router,
        &web_created["revision"],
        json!({"action":"saveApplication","id":"web","definition":definition()}),
    )
    .await;
    command(
        &app.router,
        &web_saved["revision"],
        json!({"action":"publishApplication","id":"web","note":"Initial release"}),
    )
    .await;

    let mut acme_with_web = json!({"displayName":"Acme","legalName":"Acme Ltd","region":"NZ",
        "timezone":"Pacific/Auckland","hosts":["www.example.com"],"configuration":{},
        "applications":[]});
    acme_with_web["applications"] =
        json!([{"applicationId":"web","version":1,"planId":"standard","configuration":{"team":"Finance"}}]);

    let refused = send(
        &app.router,
        as_operator("PUT", "/api/clients/acme/product")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{}\"", app.revision))
            .body(Body::from(acme_with_web.to_string()))
            .unwrap(),
    )
    .await;
    let status = refused.status();
    let body = support::json(refused).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("already uses this application identifier"),
        "{body}"
    );

    let duplicate = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":client()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    assert_eq!(
        support::json(duplicate).await["error"]["code"],
        "client_exists",
        "a taken id is not the same event as a stale revision"
    );
    let stale = send(
        &app.router,
        as_operator("PUT", "/api/clients/newco/product")
            .header("content-type", "application/json")
            .header("if-match", "\"stale\"")
            .body(Body::from(client().to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
}
#[tokio::test]
async fn catalogue_requires_auth_and_conditional_writes() {
    let app = control_plane();
    for path in [
        "/api/catalogue",
        "/api/activity",
        "/api/operator",
        "/api/clients/unknown/product",
    ] {
        let response = send(
            &app.router,
            http::Request::builder().uri(path).body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let value = json!({"action":"createApplication","id":"analytics","name":"Analytics"});
    command(&app.router, &Value::Null, value.clone()).await;
    let response = send(
        &app.router,
        as_operator("POST", "/api/catalogue")
            .header("content-type", "application/json")
            .header("if-none-match", "*")
            .body(Body::from(value.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
#[tokio::test]
async fn invalid_publication_and_unpublished_assignment_are_rejected() {
    let app = control_plane();
    let draft = command(
        &app.router,
        &Value::Null,
        json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
    )
    .await;
    let response = send(
        &app.router,
        as_operator("POST", "/api/catalogue")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{}\"", draft["revision"].as_str().unwrap()))
            .body(Body::from(
                json!({"action":"publishApplication","id":"analytics","note":"Invalid"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Add at least one plan"),
        "{body}"
    );

    let response = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":client()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Only published application versions can be assigned"),
        "resolving against the unpublished draft must fail this way, not some other: {body}"
    );
}

#[tokio::test]
async fn put_product_replaces_configuration_and_records_activity() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/product")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .body(Body::from(
                json!({
                    "displayName": "Acme",
                    "hosts": ["www.example.com"],
                    "legalName": "Acme Ltd",
                    "region": "NZ",
                    "timezone": "Pacific/Auckland",
                    "configuration": {},
                    "applications": []
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let new_revision = entity_tag(&response);
    assert_ne!(new_revision, plane.revision.to_string());

    let body = support::json(response).await;
    assert_eq!(body["client"]["revision"].as_str().unwrap(), new_revision);
    assert_eq!(
        body["product"]["activity"].as_array().unwrap().last().unwrap()["action"],
        "Client configuration updated"
    );
}

#[tokio::test]
async fn put_product_refuses_to_remove_an_assigned_application_and_leaves_the_client_unchanged() {
    // docs/delivery.md: this rule was disabled in `service/product.rs`,
    // confirmed to fail this test, and restored — see the report for the
    // agent's account of that step.
    let app = control_plane();
    let first = command(
        &app.router,
        &Value::Null,
        json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
    )
    .await;
    let saved = command(
        &app.router,
        &first["revision"],
        json!({"action":"saveApplication","id":"analytics","definition":definition()}),
    )
    .await;
    command(
        &app.router,
        &saved["revision"],
        json!({"action":"publishApplication","id":"analytics","note":"Initial release"}),
    )
    .await;

    let created = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":client()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = support::json(created).await;
    let revision = created["client"]["revision"].as_str().unwrap().to_owned();

    let mut without_application = client();
    without_application["applications"] = json!([]);

    let response = send(
        &app.router,
        as_operator("PUT", "/api/clients/newco/product")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{revision}\""))
            .body(Body::from(without_application.to_string()))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let after = send(
        &app.router,
        as_operator("GET", "/api/clients/newco/product")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let after = support::json(after).await;

    assert_eq!(after["product"]["applications"][0]["applicationId"], "analytics");
    assert_eq!(after["client"]["revision"].as_str().unwrap(), revision);
}

#[tokio::test]
async fn posting_the_catalogue_with_if_none_match_star_when_one_exists_is_conflict() {
    let app = control_plane();
    command(
        &app.router,
        &Value::Null,
        json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
    )
    .await;

    let response = send(
        &app.router,
        as_operator("POST", "/api/catalogue")
            .header("content-type", "application/json")
            .header("if-none-match", "*")
            .body(Body::from(
                json!({"action":"createApplication","id":"other","name":"Other"}).to_string(),
            ))
            .unwrap(),
    )
    .await;

    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    // Beyond `catalogue_requires_auth_and_conditional_writes`'s status-only
    // check: the code a console branches on, and proof the refused write
    // changed nothing — "other" never joined the catalogue it was refused
    // for.
    assert_eq!(body["error"]["code"], "revision_conflict");

    let catalogue = send(
        &app.router,
        as_operator("GET", "/api/catalogue").body(Body::empty()).unwrap(),
    )
    .await;
    let catalogue = support::json(catalogue).await;
    assert_eq!(
        catalogue["catalogue"]["applications"].as_array().unwrap().len(),
        1
    );
}

#[tokio::test]
async fn activity_merges_catalogue_and_client_events_newest_first() {
    // Both entries are written straight through the repository, at explicit
    // `at` values, rather than through the HTTP command flow — the test
    // fixtures share one fixed clock (`support::FixedClock`), so two events
    // produced through the API would tie on `at` and could not tell a real
    // sort from no sort at all.
    let plane = control_plane();

    let mut catalogue = Catalogue::default();
    catalogue.activity.push(ProductActivity {
        at: 100,
        operator: OPERATOR.into(),
        action: "Application created".into(),
        resource: "analytics".into(),
    });
    plane
        .repository
        .save_catalogue(&catalogue, None, &change())
        .await
        .unwrap();

    let acme = ClientId::try_new("acme").unwrap();
    let stored = plane.repository.get(&acme).await.unwrap();
    let request = ClientProductRequest {
        display_name: "Acme".into(),
        hosts: vec![Host::try_new("www.example.com").unwrap()],
        legal_name: "Acme Ltd".into(),
        region: "NZ".into(),
        timezone: "Pacific/Auckland".into(),
        configuration: std::collections::BTreeMap::default(),
        applications: vec![],
    };
    let product = ClientProduct {
        activity: vec![ProductActivity {
            at: 200,
            operator: OPERATOR.into(),
            action: "Client configuration updated".into(),
            resource: "acme".into(),
        }],
        ..ClientProduct::default()
    };
    let updated = stored.document.with_product(&request, &product).unwrap();
    plane
        .repository
        .update(&acme, &updated, &stored.revision, &change())
        .await
        .unwrap();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/activity").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = support::json(response).await;
    let activity = body["activity"].as_array().unwrap();

    assert_eq!(activity[0]["resource"], "acme", "the newer event must sort first");
    assert_eq!(activity[0]["at"].as_u64().unwrap(), 200);
    assert_eq!(activity[1]["resource"], "analytics");
    assert_eq!(activity[1]["at"].as_u64().unwrap(), 100);
}

#[tokio::test]
async fn operator_endpoint_returns_the_authenticated_subject() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/operator").body(Body::empty()).unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(support::json(response).await["subject"], OPERATOR);
}

#[tokio::test]
async fn reconciliation_does_not_touch_the_catalogue_revision() {
    // The regression this guards: `sweep` used to write a "reconciliation
    // pass completed" event into the catalogue after every convergence,
    // which bumped its revision on every `POST /api/reconciliation` —
    // including the one `converge::in_background` fires after an unrelated
    // client write — and made an operator's next catalogue save fail with a
    // conflict they never caused.
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_identity_provider(provider);

    let before = plane.repository.catalogue().await.unwrap();
    assert_eq!(before.revision, None, "nothing has written the catalogue yet");

    let response = send(
        &plane.router,
        as_operator("POST", "/api/reconciliation")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let after = plane.repository.catalogue().await.unwrap();
    assert_eq!(after.revision, None, "a sweep must not write to the catalogue");
}

/// A client document whose identity is fine but whose `spec.product` will not
/// deserialize into [`ClientProduct`] — `legalName` is a number where every
/// other required field is also absent. `ClientDocument::parse` never looks
/// inside `product`, so this document is stored successfully and fails only
/// when something later calls `.product()`.
const MALFORMED_PRODUCT_CLIENT: &str = r"apiVersion: fabric.fieldstate.nz/v1
kind: Client
metadata:
  name: broken
spec:
  displayName: Broken
  hosts:
    - broken.example.com
  identity:
    realm: broken
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients: []
  product:
    legalName: 123
";

#[tokio::test]
async fn a_stored_product_section_that_will_not_parse_is_the_platforms_problem_not_the_callers() {
    // Regression: this used to answer 400 `invalid_request`, as though the
    // caller had sent something wrong — but neither `GET .../product` nor
    // `GET /api/activity` reads anything from the request. What is broken is
    // already in the repository, which is exactly the case
    // `desired_state_invalid` exists for (see `GET /api/clients`, which
    // reports the same code for a client document that will not parse at
    // all).
    let plane = control_plane();
    plane
        .repository
        .insert(ClientDocument::parse(MALFORMED_PRODUCT_CLIENT).unwrap())
        .unwrap();

    let product = send(
        &plane.router,
        as_operator("GET", "/api/clients/broken/product")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(product.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        support::json(product).await["error"]["code"],
        "desired_state_invalid"
    );

    let activity = send(
        &plane.router,
        as_operator("GET", "/api/activity").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(activity.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        support::json(activity).await["error"]["code"],
        "desired_state_invalid"
    );
}

/// A client document declaring a realm that is not its own id — the shape
/// only a hand-edited (or otherwise API-bypassing) document can take, since
/// `ClientDocument::create` never produces one.
const HAND_EDITED_REALM: &str = r"apiVersion: fabric.fieldstate.nz/v1
kind: Client
metadata:
  name: foo
spec:
  displayName: Foo
  identity:
    realm: {realm}
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients: []
";

fn empty_client() -> Value {
    json!({"displayName":"Test","legalName":"Test Ltd","region":"NZ","timezone":"Pacific/Auckland",
        "hosts":[],"configuration":{},"applications":[]})
}

async fn create(router: &Router, id: &str) -> Response<Body> {
    send(
        router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":id,"configuration":empty_client()}).to_string(),
            ))
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn a_client_may_not_take_a_reserved_or_already_used_realm() {
    // `ClientDocument::create` sets a new client's realm to its own id, and
    // reconciliation treats any realm the document names as this client's —
    // including one that already exists for another reason. `master`,
    // the operator posture's own realm, and a realm another stored client
    // already declares are exactly the takeovers this refuses.
    let app = control_plane();

    let master = create(&app.router, "master").await;
    let master_status = master.status();
    let master_body = support::json(master).await;
    assert_eq!(master_status, StatusCode::CONFLICT, "{master_body}");
    assert_eq!(master_body["error"]["code"], "realm_unavailable");

    let operator_realm = create(&app.router, OPERATOR_REALM).await;
    assert_eq!(operator_realm.status(), StatusCode::CONFLICT);
    assert_eq!(
        support::json(operator_realm).await["error"]["code"],
        "realm_unavailable"
    );

    // `ClientDocument::create` always sets a client's realm to its own id,
    // so the only way a *different* client ends up declaring `bar` is a
    // hand-edited document — inserted directly, the way it would arrive
    // through Git. `foo`'s realm is `bar`, not `foo`.
    app.repository
        .insert(ClientDocument::parse(&HAND_EDITED_REALM.replace("{realm}", "bar")).unwrap())
        .unwrap();

    let taken = create(&app.router, "bar").await;
    assert_eq!(taken.status(), StatusCode::CONFLICT);
    assert_eq!(support::json(taken).await["error"]["code"], "realm_unavailable");

    // An ordinary id, naming no reserved or already-declared realm, is
    // still created.
    let ordinary = create(&app.router, "newco").await;
    assert_eq!(ordinary.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn an_assignment_is_refused_for_a_client_with_an_internal_host() {
    // A client's own declared host feeds the per-host callback
    // `with_application_identity` builds; `.internal` classifies as a
    // private-network host, which the `claimedHttps` strategy every
    // application client uses does not admit.
    let app = control_plane();
    let first = command(
        &app.router,
        &Value::Null,
        json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
    )
    .await;
    let saved = command(
        &app.router,
        &first["revision"],
        json!({"action":"saveApplication","id":"analytics","definition":definition()}),
    )
    .await;
    command(
        &app.router,
        &saved["revision"],
        json!({"action":"publishApplication","id":"analytics","note":"Initial release"}),
    )
    .await;

    let mut internal_client = client();
    internal_client["hosts"] = json!(["newco.internal"]);

    let response = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":internal_client}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn an_unrelated_product_save_keeps_the_clients_original_release_copy() {
    // ADR 0020 §3: a client is pinned to the exact release it was assigned,
    // not to "whatever the catalogue later says". A release edited by hand
    // in the repository — the only way it can change once published — must
    // not reach a client through some *other*, unrelated save.
    let app = control_plane();
    let first = command(
        &app.router,
        &Value::Null,
        json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
    )
    .await;
    let saved = command(
        &app.router,
        &first["revision"],
        json!({"action":"saveApplication","id":"analytics","definition":definition()}),
    )
    .await;
    command(
        &app.router,
        &saved["revision"],
        json!({"action":"publishApplication","id":"analytics","note":"Initial release"}),
    )
    .await;

    let created = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":client()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = support::json(created).await;
    let revision = created["client"]["revision"].as_str().unwrap().to_owned();
    assert_eq!(
        created["product"]["applications"][0]["release"]["definition"]["name"],
        "Analytics"
    );

    // Edited directly in the repository — bypassing every command that would
    // otherwise refuse to mutate a published release.
    let stored = app.repository.catalogue().await.unwrap();
    let mut edited = stored.catalogue.clone();
    let release = edited
        .applications
        .first_mut()
        .unwrap()
        .releases
        .first_mut()
        .unwrap();
    release.definition.name = "Edited By Hand".into();
    app.repository
        .save_catalogue(&edited, stored.revision.as_ref(), &change())
        .await
        .unwrap();

    // An unrelated save: the same application and version, only the legal
    // name changes.
    let mut unrelated = client();
    unrelated["legalName"] = json!("Newco Holdings Ltd");

    let response = send(
        &app.router,
        as_operator("PUT", "/api/clients/newco/product")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{revision}\""))
            .body(Body::from(unrelated.to_string()))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    assert_eq!(body["product"]["legalName"], "Newco Holdings Ltd");
    assert_eq!(
        body["product"]["applications"][0]["release"]["definition"]["name"], "Analytics",
        "the client's own copy must not have moved just because the catalogue's did"
    );
}

/// Shared by the tests above that write to the repository directly rather
/// than through an HTTP command.
fn change() -> ChangeContext {
    ChangeContext {
        requested_by: OPERATOR.into(),
        summary: "test fixture".into(),
    }
}
