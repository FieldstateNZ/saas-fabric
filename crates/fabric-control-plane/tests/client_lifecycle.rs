//! A client's lifecycle through the real router (D01-14, D01-19a): saved
//! intent until provisioning, identity and publication are met; a tombstone
//! that records retirement, retains state and leaves the tenant published
//! (withdrawal is deferred) with its withdrawal reported only from
//! publication evidence; a separate deletion confirmation that records intent
//! only; and every refusal.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::indexing_slicing
)]

mod support;

use axum::body::Body;
use fabric_client_model::{ClientDocument, ClientId, ClientRevision};
use fabric_control_plane::ClientRepository;
use fabric_core::DataSourceId;
use fabric_platform_management::{DesiredRevision, PlacementState};
use fabric_reconciliation::ReconciliationOutcome;
use http::StatusCode;
use serde_json::{json, Value};
use support::lifecycle::{
    conditions, confirm_deletion, declared_data_source, get, placement, post,
    publish_one_application_with_a_resource, sink, tenants_document, tenants_published, tombstone, GLOBEX,
};
use support::platform_fixture::{platform_binding, FakeRepository};
use support::{as_operator, control_plane, control_plane_with_publication, json as body_of, send, TempDir};

async fn placed_tenants(fake: &FakeRepository) -> Vec<String> {
    fake.read_placements("lucentroot")
        .await
        .unwrap()
        .placements
        .iter()
        .map(|record| record.tenant.as_str().to_owned())
        .collect()
}

// Saved intent and readiness.

#[tokio::test]
async fn a_client_is_saved_intent_until_every_readiness_condition_is_met() {
    let plane = control_plane();

    let (status, body) = get(&plane.router, "/api/clients/acme").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["lifecycle"]["state"], "incomplete", "{body}");
    assert_eq!(
        conditions(&body["lifecycle"]),
        ["provisioning", "identity", "publication"]
    );
    assert_eq!(body["lifecycle"]["tombstone"], Value::Null);
    assert_eq!(body["lifecycle"]["deletionConfirmed"], Value::Null);

    let (_, list) = get(&plane.router, "/api/clients").await;
    assert_eq!(list["clients"][0]["lifecycle"]["state"], "incomplete", "{list}");
    let (_, product) = get(&plane.router, "/api/clients/acme/product").await;
    assert_eq!(product["client"]["lifecycle"]["state"], "incomplete", "{product}");
}

#[tokio::test]
async fn a_client_becomes_ready_once_placed_applied_and_published_and_retirement_leaves_every_tenant_published(
) {
    let dir = TempDir::new("lifecycle-ready");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    plane
        .repository
        .insert(&ClientDocument::parse(GLOBEX).unwrap())
        .unwrap();
    let revision = plane.enroll("globex").await;

    let (_, saved) = get(&plane.router, "/api/clients/globex").await;
    assert_eq!(saved["lifecycle"]["state"], "incomplete", "{saved}");
    assert_eq!(
        saved["lifecycle"]["unmet"][0]["detail"],
        "not placed yet: primary"
    );
    assert_eq!(
        saved["lifecycle"]["withdrawal"],
        Value::Null,
        "only retirement reports withdrawal"
    );

    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(
        DesiredRevision::new("pl-1"),
        vec![placement("acme"), placement("globex")],
    );
    let (_, placed) = get(&plane.router, "/api/clients/globex").await;
    assert_eq!(
        conditions(&placed["lifecycle"]),
        ["identity", "publication"],
        "{placed}"
    );

    plane.statuses.record(
        &ClientId::try_new("globex").unwrap(),
        revision.clone(),
        &ReconciliationOutcome::converged(),
        1,
    );
    publish_one_application_with_a_resource(&plane.router).await;
    let (status, pass) = post(&plane.router, "/api/platform/publication", None, &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{pass}");
    let before = tenants_document(&dir);
    assert_eq!(tenants_published(&dir), ["acme", "globex"]);

    let (_, ready) = get(&plane.router, "/api/clients/globex").await;
    assert_eq!(ready["lifecycle"]["state"], "ready", "{ready}");
    assert_eq!(ready["lifecycle"]["unmet"], json!([]));

    let (status, tombstoned) = tombstone(&plane.router, "globex", revision.as_str()).await;
    assert_eq!(status, StatusCode::OK, "{tombstoned}");
    assert_eq!(tombstoned["lifecycle"]["state"], "tombstoned", "{tombstoned}");
    assert_eq!(tombstoned["lifecycle"]["tombstone"]["reason"], "Contract ended");
    assert_eq!(tombstoned["lifecycle"]["tombstone"]["by"], support::OPERATOR);
    assert_eq!(tombstoned["lifecycle"]["deletionConfirmed"], Value::Null);
    assert_eq!(
        tombstoned["lifecycle"]["withdrawal"]["state"], "pending",
        "recording retirement is not withdrawal: {tombstoned}"
    );

    // Retirement changes no placement and no tenant binding, so the next
    // pass offers the same tenants document. That is what lets a history
    // guard that refuses a removed or lowered published binding (#138)
    // admit it: nothing published is removed, and every other tenant's
    // publication proceeds.
    let (status, pass) = post(&plane.router, "/api/platform/publication", None, &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{pass}");
    let outcome = pass["lastPass"]["outcome"].as_str().unwrap();
    assert!(outcome == "published" || outcome == "unchanged", "{pass}");
    assert_eq!(
        tenants_document(&dir),
        before,
        "the retired tenant's binding is kept as it was"
    );
    assert_eq!(
        placed_tenants(&fake).await,
        ["acme", "globex"],
        "its placement is retained"
    );

    let (_, after_pass) = get(&plane.router, "/api/clients/globex").await;
    assert_eq!(
        after_pass["lifecycle"]["withdrawal"]["state"], "pending",
        "{after_pass}"
    );
    let detail = after_pass["lifecycle"]["withdrawal"]["detail"].as_str().unwrap();
    assert!(detail.contains("still carries this tenant"), "{detail}");
    let (_, other) = get(&plane.router, "/api/clients/acme").await;
    assert_eq!(other["lifecycle"]["withdrawal"], Value::Null, "{other}");

    let tombstoned_revision = tombstoned["revision"].as_str().unwrap();
    let (status, confirmed) = confirm_deletion(&plane.router, "globex", tombstoned_revision).await;
    assert_eq!(status, StatusCode::OK, "{confirmed}");
    assert_eq!(
        confirmed["lifecycle"]["state"], "deletionConfirmed",
        "{confirmed}"
    );
    assert_eq!(
        confirmed["lifecycle"]["deletionConfirmed"]["by"],
        support::OPERATOR
    );
    assert_eq!(confirmed["lifecycle"]["tombstone"]["reason"], "Contract ended");
    assert_eq!(
        confirmed["lifecycle"]["withdrawal"]["state"], "pending",
        "{confirmed}"
    );

    // Confirmation records intent only: the document, its data intent, its
    // placement and its published binding are all still there.
    post(&plane.router, "/api/platform/publication", None, &json!({})).await;
    assert_eq!(tenants_document(&dir), before);
    let (status, retained) = get(&plane.router, "/api/clients/globex/placements").await;
    assert_eq!(status, StatusCode::OK, "{retained}");
    assert_eq!(placed_tenants(&fake).await, ["acme", "globex"]);
    let stored = plane
        .repository
        .get(&ClientId::try_new("globex").unwrap())
        .await
        .unwrap();
    assert!(stored.document.render().unwrap().contains("class: shared"));
}

#[tokio::test]
async fn retiring_the_only_published_tenant_neither_empties_nor_stalls_publication() {
    let dir = TempDir::new("lifecycle-only-tenant");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(DesiredRevision::new("pl-1"), vec![placement("acme")]);
    publish_one_application_with_a_resource(&plane.router).await;
    post(&plane.router, "/api/platform/publication", None, &json!({})).await;
    assert_eq!(tenants_published(&dir), ["acme"]);

    let (status, tombstoned) = tombstone(&plane.router, "acme", plane.revision.as_str()).await;
    assert_eq!(status, StatusCode::OK);
    let (_, pass) = post(&plane.router, "/api/platform/publication", None, &json!({})).await;

    assert_eq!(pass["lastPass"]["outcome"], "unchanged", "{pass}");
    assert_eq!(
        tenants_published(&dir),
        ["acme"],
        "still served: withdrawal is deferred"
    );
    assert_eq!(
        tombstoned["lifecycle"]["withdrawal"]["state"], "pending",
        "{tombstoned}"
    );
    let (_, after) = get(&plane.router, "/api/clients/acme").await;
    assert_eq!(after["lifecycle"]["withdrawal"]["state"], "pending", "{after}");
}

#[tokio::test]
async fn a_refused_pass_is_reported_as_refused_and_the_last_good_tenant_stays() {
    let dir = TempDir::new("lifecycle-refused");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(DesiredRevision::new("pl-1"), vec![placement("acme")]);
    publish_one_application_with_a_resource(&plane.router).await;
    post(&plane.router, "/api/platform/publication", None, &json!({})).await;
    tombstone(&plane.router, "acme", plane.revision.as_str()).await;

    // A placement on a data source nobody declared: the pass refuses it.
    let mut stray = placement("acme");
    stray.data_source = DataSourceId::try_new("undeclared").unwrap();
    fake.seed_placements(DesiredRevision::new("pl-2"), vec![stray]);
    let (_, pass) = post(&plane.router, "/api/platform/publication", None, &json!({})).await;
    let outcome = pass["lastPass"]["outcome"].as_str().unwrap();
    assert!(outcome == "refused" || outcome == "failed", "{pass}");

    let (_, after) = get(&plane.router, "/api/clients/acme").await;
    assert_eq!(after["lifecycle"]["withdrawal"]["state"], "refused", "{after}");
    assert_eq!(tenants_published(&dir), ["acme"], "the last good document stays");
}

#[tokio::test]
async fn without_publication_retirement_reports_withdrawal_as_unobserved() {
    let plane = control_plane();

    let (status, tombstoned) = tombstone(&plane.router, "acme", plane.revision.as_str()).await;

    assert_eq!(status, StatusCode::OK, "{tombstoned}");
    assert_eq!(
        tombstoned["lifecycle"]["withdrawal"]["state"], "unobserved",
        "{tombstoned}"
    );
}

// Refused transitions.

#[tokio::test]
async fn deletion_cannot_be_confirmed_without_a_tombstone() {
    let plane = control_plane();

    let (status, body) = confirm_deletion(&plane.router, "acme", plane.revision.as_str()).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "deletion_requires_tombstone");
    let (_, after) = get(&plane.router, "/api/clients/acme").await;
    assert_eq!(after["revision"], plane.revision.as_str(), "nothing was written");
}

#[tokio::test]
async fn deletion_is_confirmed_by_its_own_write_not_by_the_tombstone() {
    let plane = control_plane();

    let (_, tombstoned) = tombstone(&plane.router, "acme", plane.revision.as_str()).await;
    assert_eq!(tombstoned["lifecycle"]["state"], "tombstoned");
    assert_eq!(tombstoned["lifecycle"]["deletionConfirmed"], Value::Null);

    // The confirmation must name the revision the tombstone produced: the
    // revision it was taken from no longer exists.
    let (status, body) = confirm_deletion(&plane.router, "acme", plane.revision.as_str()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "revision_conflict");
}

#[tokio::test]
async fn a_tombstone_body_cannot_carry_a_deletion_confirmation() {
    let plane = control_plane();

    let (status, body) = post(
        &plane.router,
        "/api/clients/acme/tombstone",
        Some(plane.revision.as_str()),
        &json!({"reason": "Contract ended", "confirmClientId": "acme"}),
    )
    .await;

    assert!(status.is_client_error(), "{status} {body}");
    let (_, after) = get(&plane.router, "/api/clients/acme").await;
    assert_eq!(after["lifecycle"]["state"], "incomplete");
}

#[tokio::test]
async fn lifecycle_writes_need_the_current_revision() {
    let plane = control_plane();

    let (status, body) = post(
        &plane.router,
        "/api/clients/acme/tombstone",
        None,
        &json!({"reason": "Contract ended"}),
    )
    .await;
    assert_eq!(status, StatusCode::PRECONDITION_REQUIRED, "{body}");

    let stale = ClientRevision::try_new("stale").unwrap();
    let (status, body) = tombstone(&plane.router, "acme", stale.as_str()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "revision_conflict");
}

#[tokio::test]
async fn a_tombstone_needs_a_reason() {
    let plane = control_plane();

    let (status, body) = post(
        &plane.router,
        "/api/clients/acme/tombstone",
        Some(plane.revision.as_str()),
        &json!({"reason": "  "}),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "invalid_tombstone_reason");
}

#[tokio::test]
async fn a_client_cannot_be_tombstoned_twice() {
    let plane = control_plane();
    let (_, first) = tombstone(&plane.router, "acme", plane.revision.as_str()).await;

    let (status, body) = tombstone(&plane.router, "acme", first["revision"].as_str().unwrap()).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "client_already_tombstoned");
}

#[tokio::test]
async fn a_confirmation_naming_another_client_is_refused() {
    let plane = control_plane();
    let (_, tombstoned) = tombstone(&plane.router, "acme", plane.revision.as_str()).await;

    let (status, body) = post(
        &plane.router,
        "/api/clients/acme/deletion-confirmation",
        Some(tombstoned["revision"].as_str().unwrap()),
        &json!({"confirmClientId": "globex"}),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "deletion_confirmation_mismatch");
}

#[tokio::test]
async fn deletion_confirmed_is_final() {
    let plane = control_plane();
    let (_, tombstoned) = tombstone(&plane.router, "acme", plane.revision.as_str()).await;
    let (_, confirmed) =
        confirm_deletion(&plane.router, "acme", tombstoned["revision"].as_str().unwrap()).await;
    let revision = confirmed["revision"].as_str().unwrap();

    let (status, body) = confirm_deletion(&plane.router, "acme", revision).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "deletion_already_confirmed");

    let (status, body) = tombstone(&plane.router, "acme", revision).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "deletion_already_confirmed");
}

#[tokio::test]
async fn an_unknown_client_cannot_be_tombstoned() {
    let plane = control_plane();

    let (status, body) = tombstone(&plane.router, "initech", plane.revision.as_str()).await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn lifecycle_writes_need_an_operator() {
    let plane = control_plane();
    let response = send(
        &plane.router,
        http::Request::builder()
            .method("POST")
            .uri("/api/clients/acme/tombstone")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{}\"", plane.revision))
            .body(Body::from(json!({"reason": "x"}).to_string()))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// What a retired client refuses, and what it still allows.

#[tokio::test]
async fn a_tombstoned_client_refuses_product_edits_and_secret_removal() {
    let plane = control_plane();
    let (_, tombstoned) = tombstone(&plane.router, "acme", plane.revision.as_str()).await;
    let revision = tombstoned["revision"].as_str().unwrap();

    let response = send(
        &plane.router,
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
    let status = response.status();
    let body = body_of(response).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "client_retired");
    assert_eq!(
        body["error"]["message"],
        "the client is tombstoned, so editing is refused; its state is retained"
    );

    let response = send(
        &plane.router,
        as_operator("DELETE", "/api/clients/acme/secrets/entry/database/primary")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = body_of(response).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "client_retired");
}

#[tokio::test]
async fn a_tombstoned_client_refuses_placement() {
    let (platform, fake) = platform_binding().await;
    let dir = TempDir::new("lifecycle-placement");
    let plane = control_plane_with_publication(platform, sink(&dir));
    plane
        .repository
        .insert(&ClientDocument::parse(GLOBEX).unwrap())
        .unwrap();
    let revision = plane.enroll("globex").await;
    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(DesiredRevision::new("pl-1"), vec![]);
    tombstone(&plane.router, "globex", revision.as_str()).await;

    let response = send(
        &plane.router,
        as_operator("POST", "/api/clients/globex/placements/primary")
            .header("if-match", "\"pl-1\"")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = body_of(response).await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "client_retired");
    assert_eq!(fake.placement_writes(), 0);
}

#[tokio::test]
async fn a_tombstoned_clients_identity_can_still_be_remediated() {
    // A tombstone does not revoke identity (that is not decided), so the
    // edit that removes a compromised redirect URI or role stays possible.
    let plane = control_plane();
    let (_, tombstoned) = tombstone(&plane.router, "acme", plane.revision.as_str()).await;

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header("content-type", "application/json")
            .header(
                "if-match",
                format!("\"{}\"", tombstoned["revision"].as_str().unwrap()),
            )
            .body(Body::from(
                json!({
                    "realm": "acme",
                    "roles": ["Client Realm Administrator", "Client Realm User"],
                    "clients": [],
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let (_, after) = get(&plane.router, "/api/clients/acme").await;
    assert_eq!(
        after["lifecycle"]["state"], "tombstoned",
        "the tombstone survives the edit"
    );
}
