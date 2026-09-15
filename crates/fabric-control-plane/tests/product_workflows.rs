//! Product workflows exercise authentication, immutable releases and real client documents.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod support;
use std::sync::Arc;

use axum::{body::Body, Router};
use fabric_client_model::catalogue::{
    Catalogue, ClientProduct, ClientProductRequest, ConfigurationField, FieldKind, ProductActivity,
};
use fabric_client_model::{ClientDocument, ClientId, Host};
use fabric_control_plane::{ChangeContext, ClientRepository};
use fabric_reconciliation::testing::FakeIdentityProvider;
use http::{header, Response, StatusCode};
use serde_json::{json, Value};
use support::{
    as_operator, control_plane, control_plane_with_identity_provider, entity_tag, send, OPERATOR,
    OPERATOR_REALM, RESERVED_APPLICATION_ID,
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
        .insert(&ClientDocument::parse(MALFORMED_PRODUCT_CLIENT).unwrap())
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

    // The status and code alone would still pass if the realm check ran
    // *after* `repository.create` — refusing the response while `master`
    // was already written. Nothing must actually exist at that id.
    let master_get = send(
        &app.router,
        as_operator("GET", "/api/clients/master")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let master_get_status = master_get.status();
    let master_get_body = support::json(master_get).await;
    assert_eq!(master_get_status, StatusCode::NOT_FOUND, "{master_get_body}");

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
        .insert(&ClientDocument::parse(&HAND_EDITED_REALM.replace("{realm}", "bar")).unwrap())
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
async fn a_deployment_reserved_application_id_is_refused() {
    // `reserved_client_ids` — this deployment's own OIDC client ids, the
    // console's and a converged Keycloak's — is opaque to the control
    // plane and computed at the composition root (see `ClientService`'s
    // own field for why never a `ClientId`). The test harness fixes
    // `RESERVED_APPLICATION_ID` as what a real deployment's own console id
    // would be, so this test drives the real router against a genuine
    // deployment-reserved id — not only the static built-ins
    // `fabric-client-model` refuses on its own, which is all an empty set
    // here would ever have been able to prove.
    let app = control_plane();

    let response = send(
        &app.router,
        as_operator("POST", "/api/catalogue")
            .header("content-type", "application/json")
            .header("if-none-match", "*")
            .body(Body::from(
                json!({"action":"createApplication","id":RESERVED_APPLICATION_ID,"name":"Shadow Console"})
                    .to_string(),
            ))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_request", "{body}");
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
async fn a_client_id_too_long_for_its_affixed_hostname_label_is_refused_at_assignment() {
    // The catalogue-wide hostname check (`validate_domain`) only refuses a
    // template that could never publish for *any* client — see
    // `hostname.rs`'s own rustdoc for why it no longer checks every
    // template against a bare 63-character worst case. A client whose own
    // id genuinely does not fit this template's label is a per-client
    // problem, refused here, at the one place that id is actually
    // substituted in.
    let app = control_plane();
    let mut affixed = definition();
    affixed["domain"] = json!("{client}-portal.example.com");
    let first = command(
        &app.router,
        &Value::Null,
        json!({"action":"createApplication","id":"analytics","name":"Analytics"}),
    )
    .await;
    let saved = command(
        &app.router,
        &first["revision"],
        json!({"action":"saveApplication","id":"analytics","definition":affixed}),
    )
    .await;
    command(
        &app.router,
        &saved["revision"],
        json!({"action":"publishApplication","id":"analytics","note":"Initial release"}),
    )
    .await;

    // 60 characters: a legal `ClientId` on its own (the limit is 63), but
    // combined with "-portal" (7 characters) the label this domain
    // substitutes into would be 67 — over the DNS label limit.
    let long_id = "a".repeat(60);

    let response = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id": long_id, "configuration": client()}).to_string(),
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

#[tokio::test]
async fn upgrading_an_assignments_version_takes_the_new_releases_copy() {
    // `resolve()` keeps a stored copy only when an assignment's
    // application id *and* version are both unchanged. Every other test in
    // this file assigns version 1 throughout, which would still pass if
    // that check only compared the application id and ignored the
    // version — this is the one that would not.
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
    let published_v1 = command(
        &app.router,
        &saved["revision"],
        json!({"action":"publishApplication","id":"analytics","note":"v1"}),
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

    // A new draft, distinguishable from v1's, published as v2.
    let mut v2_definition = definition();
    v2_definition["name"] = json!("Analytics V2");
    let saved_v2 = command(
        &app.router,
        &published_v1["revision"],
        json!({"action":"saveApplication","id":"analytics","definition":v2_definition}),
    )
    .await;
    command(
        &app.router,
        &saved_v2["revision"],
        json!({"action":"publishApplication","id":"analytics","note":"v2"}),
    )
    .await;

    let mut upgraded = client();
    upgraded["applications"] = json!([{"applicationId":"analytics","version":2,"planId":"standard","configuration":{"team":"Finance"}}]);

    let response = send(
        &app.router,
        as_operator("PUT", "/api/clients/newco/product")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{revision}\""))
            .body(Body::from(upgraded.to_string()))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["product"]["applications"][0]["release"]["definition"]["name"], "Analytics V2",
        "upgrading the assignment's version must take the new release's own copy, not keep the old one"
    );
}

#[tokio::test]
async fn an_unrelated_save_succeeds_even_when_the_kept_releases_plan_was_removed_from_the_catalogue() {
    // The plan check inside `resolve()` runs against the *kept* copy's own
    // plans when an assignment's application and version are unchanged —
    // never re-looked up from the catalogue's current state. A hand-edit
    // that drains the catalogue's own copy of the release's plans must not
    // reach an already-assigned client through some later, unrelated save.
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

    // Edited directly in the repository — a hand-edit no command exposes:
    // the published release's own "standard" plan, renamed. A published
    // release must still carry at least one plan to validate, so this
    // renames it rather than draining the list, which is enough either
    // way to make "standard" no longer part of the catalogue's own copy.
    let stored = app.repository.catalogue().await.unwrap();
    let mut edited = stored.catalogue.clone();
    edited
        .applications
        .first_mut()
        .unwrap()
        .releases
        .first_mut()
        .unwrap()
        .definition
        .plans
        .first_mut()
        .unwrap()
        .id = ClientId::try_new("renamed").unwrap();
    app.repository
        .save_catalogue(&edited, stored.revision.as_ref(), &change())
        .await
        .unwrap();

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
    assert_eq!(body["product"]["applications"][0]["planId"], "standard");
}

#[tokio::test]
async fn an_unrelated_save_succeeds_even_when_the_kept_releases_field_was_renamed_in_the_catalogue() {
    // `resolve()`'s configuration check runs against the same `release` its
    // plan check does — `values(&release.definition.fields, ...)` — so the
    // kept-copy direction matters here too. Every other test in this file
    // leaves the kept copy and the catalogue's copy carrying the same
    // fields, which would still pass if the check ran against the wrong
    // one; renaming the field only in the catalogue's own copy is what
    // makes the two actually disagree.
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
        created["product"]["applications"][0]["configuration"]["team"], "Finance",
        "{created}"
    );

    // Edited directly in the repository — a hand-edit no command exposes:
    // the published release's own "team" field, renamed. The stored
    // assignment's "team":"Finance" configuration is now undeclared
    // against the catalogue's copy of the release, but still declared
    // against the kept one.
    let stored = app.repository.catalogue().await.unwrap();
    let mut edited = stored.catalogue.clone();
    edited
        .applications
        .first_mut()
        .unwrap()
        .releases
        .first_mut()
        .unwrap()
        .definition
        .fields
        .first_mut()
        .unwrap()
        .key = "department".into();
    app.repository
        .save_catalogue(&edited, stored.revision.as_ref(), &change())
        .await
        .unwrap();

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
    assert_eq!(
        body["product"]["applications"][0]["configuration"]["team"],
        "Finance"
    );
}

/// A padding entry for a client's or the catalogue's own activity feed.
/// Not something a later write resubmits — `activity` is server-appended,
/// never part of a request body — so padding through it, unlike through a
/// field a request would have to carry, never runs into the 64 KiB
/// request-body limit no matter how large the *stored* document gets.
fn padding_activity(bytes: usize) -> ProductActivity {
    ProductActivity {
        at: 0,
        operator: OPERATOR.into(),
        action: "Padding".into(),
        resource: "a".repeat(bytes),
    }
}

/// The catalogue is grown directly through the repository for the same
/// reason the client-document size tests below are: a single request body
/// is capped at 64 KiB, so no HTTP call could carry enough padding in one
/// go to reach the 900 KiB limit — but `activity` accumulates the same way
/// a real sequence of catalogue commands would grow it, one entry per
/// write, just faster. What runs through the real router is the write that
/// actually matters: a catalogue command, sent as an ordinary operator
/// would send it, refused once the document it would produce is already
/// over the limit.
#[tokio::test]
async fn a_catalogue_saved_until_it_is_refused_for_size() {
    let app = control_plane();
    let current = app.repository.catalogue().await.unwrap();

    let mut catalogue = current.catalogue.clone();
    loop {
        catalogue.activity.push(padding_activity(50_000));

        if catalogue.render().unwrap().len() > 900 * 1024 {
            break;
        }
    }

    let revision = app
        .repository
        .save_catalogue(&catalogue, current.revision.as_ref(), &change())
        .await
        .unwrap();

    // Every catalogue command appends its own activity entry, so even a
    // save that changes nothing about the settings' own values still
    // grows the document further — and must be refused now that it is
    // already over the limit.
    let response = send(
        &app.router,
        as_operator("POST", "/api/catalogue")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{revision}\""))
            .body(Body::from(
                json!({
                    "action": "saveSettings",
                    "settings": {
                        "platformName": catalogue.settings.platform_name,
                        "defaultRegion": catalogue.settings.default_region,
                        "timezone": catalogue.settings.timezone,
                    },
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "document_too_large", "{body}");
}

/// `create_client` checks the ordinary limit too, same as `set_product` and
/// `change_catalogue` — deleting that one check fails nothing else, so it
/// needs its own test.
///
/// Grown through the catalogue's own custom-field defaults rather than
/// through the created client's request body, for the same reason the
/// other size tests seed directly: a single request body is capped at
/// 64 KiB, far short of the limit this test needs to cross, but a
/// *default* a client never submits is not part of that body at all — only
/// the resolved value it fills in is, on the far side of the request.
#[tokio::test]
async fn a_create_whose_resolved_document_is_too_large_is_refused() {
    let app = control_plane();
    let stored = app.repository.catalogue().await.unwrap();

    let mut catalogue = stored.catalogue.clone();
    catalogue.client_fields = (0..230)
        .map(|i| ConfigurationField {
            key: format!("field{i}"),
            label: "Padding".into(),
            kind: FieldKind::Text,
            required: false,
            default: Some("a".repeat(4096)),
            options: vec![],
            description: String::new(),
        })
        .collect();
    app.repository
        .save_catalogue(&catalogue, stored.revision.as_ref(), &change())
        .await
        .unwrap();

    // An empty submission: every field's default fills in unasked, and
    // that is enough on its own to cross the limit.
    let response = send(
        &app.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":empty_client()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = support::json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "document_too_large", "{body}");
}

/// The client document's size sits behind two different limits depending on
/// which write produced it (see `document_size`'s own rustdoc): an
/// ordinary write is refused past 900 KiB, but an identity edit — the write
/// security remediation depends on — is allowed up to 960 KiB.
///
/// The document under test is padded, and seeded, directly through the
/// repository rather than through a sequence of HTTP writes: a single
/// request body is capped at 64 KiB, so no HTTP call could carry enough
/// padding in one go to reach either limit. Padding through
/// `product.activity` keeps that true regardless — an entry there is never
/// part of what either edit below resubmits, so both of their own request
/// bodies stay small how ever large the stored document grows.
#[tokio::test]
async fn a_document_between_the_two_size_limits_blocks_a_product_save_but_not_an_identity_edit() {
    let app = control_plane();
    let acme = ClientId::try_new("acme").unwrap();
    let current = app.repository.get(&acme).await.unwrap();

    // Twenty-thousand-byte steps: fine enough, against the ~60 KiB window
    // between the two limits, that a step cannot cross both at once, and
    // few enough (about forty-six) that padding stays fast — each step
    // re-renders and re-parses the whole document so far.
    let mut padded = current.document.clone();
    loop {
        padded = padded.with_activity(padding_activity(20_000)).unwrap();

        if padded.render().unwrap().len() > 900 * 1024 {
            break;
        }
    }
    assert!(
        padded.render().unwrap().len() < 960 * 1024,
        "the seeded document overshot the remediation limit; reduce the padding step"
    );

    let revision = app
        .repository
        .update(&acme, &padded, &current.revision, &change())
        .await
        .unwrap();

    let save_response = send(
        &app.router,
        as_operator("PUT", "/api/clients/acme/product")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{revision}\""))
            .body(Body::from(
                json!({
                    "displayName": "Acme", "hosts": ["www.example.com"], "legalName": "Acme Ltd",
                    "region": "NZ", "timezone": "Pacific/Auckland", "configuration": {}, "applications": []
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    let save_status = save_response.status();
    let save_body = support::json(save_response).await;
    assert_eq!(save_status, StatusCode::UNPROCESSABLE_ENTITY, "{save_body}");
    assert_eq!(save_body["error"]["code"], "document_too_large", "{save_body}");

    // The failed save above did not move the revision: the document is
    // still exactly the size that just refused a product save, and an
    // identity edit at that same size — a tiny, ordinary one, adding no
    // padding of its own — must still succeed.
    let identity_response = send(
        &app.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{revision}\""))
            .body(Body::from(
                json!({
                    "realm": "acme",
                    "roles": ["Client Realm Administrator", "Client Realm User", "Extra Role"],
                    "clients": [],
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    let identity_status = identity_response.status();
    let identity_body = support::json(identity_response).await;
    assert_eq!(identity_status, StatusCode::OK, "{identity_body}");
}

/// Shared by the tests above that write to the repository directly rather
/// than through an HTTP command.
fn change() -> ChangeContext {
    ChangeContext {
        requested_by: OPERATOR.into(),
        summary: "test fixture".into(),
    }
}
