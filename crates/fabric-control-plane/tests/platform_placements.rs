//! `/api/clients/{clientId}/placements` and `/api/platform/data-sources/{id}`
//! `DELETE`, through the real router (ADR 0023 part 2).
//!
//! One end-to-end sequence rather than one test per outcome, for the same
//! reason `platform_data_sources.rs` is: a "placed" only proves anything
//! once a "not placed" precedes it, and "refused, already somebody else's"
//! only proves anything once a first placement has actually landed.
//! `docs/delivery.md` calls this the mandatory HTTP-surface test for this
//! slice.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]

mod support;

use axum::body::Body;
use fabric_client_model::ClientDocument;
use http::{Request, StatusCode};
use serde_json::{json, Value};
use support::platform_fixture::platform_binding;
use support::{as_operator, control_plane_with_platform, entity_tag, json as body_of, send};

const DATA_SOURCES_PATH: &str = "/api/platform/data-sources";

/// "acme", carrying a shared `primary` intent and a dedicated `audit`
/// intent -- overwrites the fixture's plain "acme", which has neither.
const ACME_WITH_DATA: &str = r"
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
  data:
    primary:
      class: shared
      provider: postgres
      region: nz
    audit:
      class: dedicated
    secondary:
      class: high_availability
";

/// "globex", carrying the same two intents so it can land on the same
/// shared source as "acme" and be refused the same dedicated one.
const GLOBEX_WITH_DATA: &str = r"
apiVersion: fabric.fieldstate.nz/v1
kind: Client
metadata:
  name: globex
spec:
  displayName: Globex
  hosts:
    - www.globex.example.com
  secrets:
    namespace: globex
  identity:
    realm: globex
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients:
      - id: web
        type: oidc
        redirectUris:
          - https://www.globex.example.com/callback
  data:
    primary:
      class: shared
      provider: postgres
      region: nz
    audit:
      class: dedicated
";

/// A shared, named-connection declaration with a discriminator column --
/// what both clients' `primary` intent admits.
fn shared_declaration() -> Value {
    json!({
        "connector": "postgres-nz",
        "connection": {"kind": "named", "name": "shared"},
        "placement": "shared",
        "residency": {"region": "nz", "jurisdiction": "NZ"},
        "pool": {"maxConnections": 20, "idleTimeoutSeconds": 300, "acquireTimeoutSeconds": 5},
        "capabilities": {"writable": true, "acceptsNewTenants": true},
        "discriminator": {"column": "tenant_key"},
        "labels": {},
    })
}

/// A dedicated declaration -- one tenant's, what both clients' `audit`
/// intent admits, but only the first tenant to ask gets it.
fn dedicated_declaration() -> Value {
    json!({
        "connector": "postgres-nz",
        "connection": {"kind": "named", "name": "audit"},
        "placement": "dedicated",
        "residency": {"region": "nz", "jurisdiction": "NZ"},
        "pool": {"maxConnections": 5, "idleTimeoutSeconds": 300, "acquireTimeoutSeconds": 5},
        "capabilities": {"writable": true, "acceptsNewTenants": true},
        "discriminator": null,
        "labels": {},
    })
}

/// A declaration nothing ever asks for, to prove `DELETE` succeeds on a
/// data source no placement names.
fn unused_declaration() -> Value {
    json!({
        "connector": "postgres-nz",
        "connection": {"kind": "named", "name": "unused"},
        "placement": "development",
        "residency": {"region": "nz", "jurisdiction": "NZ"},
        "pool": {"maxConnections": 5, "idleTimeoutSeconds": 300, "acquireTimeoutSeconds": 5},
        "capabilities": {"writable": true, "acceptsNewTenants": true},
        "discriminator": null,
        "labels": {},
    })
}

fn get(path: &str) -> Request<Body> {
    as_operator("GET", path)
        .body(Body::empty())
        .expect("the request must build")
}

fn put_data_source(path: &str, precondition: &str, body: &Value) -> Request<Body> {
    as_operator("PUT", path)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header("If-Match", precondition)
        .body(Body::from(body.to_string()))
        .expect("the request must build")
}

fn delete_data_source(path: &str, precondition: &str) -> Request<Body> {
    as_operator("DELETE", path)
        .header("If-Match", precondition)
        .body(Body::empty())
        .expect("the request must build")
}

fn post_place(path: &str, precondition: &str) -> Request<Body> {
    as_operator("POST", path)
        .header("If-Match", precondition)
        .body(Body::empty())
        .expect("the request must build")
}

/// The placement row named `logical` within a `placements` body,
/// regardless of where the list sorted it.
fn row<'a>(body: &'a Value, logical: &str) -> &'a Value {
    body["placements"]
        .as_array()
        .expect("placements must be an array")
        .iter()
        .find(|item| item["logical"] == logical)
        .unwrap_or_else(|| panic!("no logical data source named {logical} in {body}"))
}

#[tokio::test]
async fn a_clients_data_intent_is_placed_recorded_and_protects_its_data_source() {
    let (platform, _fake) = platform_binding().await;
    let plane = control_plane_with_platform(platform);

    plane
        .repository
        .insert(&ClientDocument::parse(ACME_WITH_DATA).expect("the acme fixture must parse"))
        .expect("the acme fixture must store");
    plane
        .repository
        .insert(&ClientDocument::parse(GLOBEX_WITH_DATA).expect("the globex fixture must parse"))
        .expect("the globex fixture must store");

    // 1. Nothing is declared yet, so acme's shared intent is refused, and
    // the message says why -- `placed` and `refusal` never both carry
    // something.
    let response = send(&plane.router, get("/api/clients/acme/placements")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let initial_placements_tag = entity_tag(&response);
    let body = body_of(response).await;
    assert_eq!(body["clientId"], "acme");
    assert_eq!(body["environment"], "lucentroot");
    assert_eq!(body["revision"], initial_placements_tag);
    let primary = row(&body, "primary");
    assert_eq!(primary["placed"], Value::Null);
    assert_eq!(
        primary["refusal"],
        "declare a shared data source in region nz that accepts new tenants ('postgres' was stated as a provider, but nothing declares one yet, so it was not matched)",
        "{body}"
    );

    // The intent itself renders in the console's own words: `class` is the
    // camelCase spelling `console_word` gives it (not the wire's
    // `snake_case`), and a `provider`/`region` nothing stated renders as
    // JSON `null`, not an absent key.
    assert_eq!(primary["intent"]["class"], "shared");
    assert_eq!(primary["intent"]["provider"], "postgres");
    assert_eq!(primary["intent"]["region"], "nz");
    let secondary = row(&body, "secondary");
    assert_eq!(secondary["intent"]["class"], "highAvailability");
    assert_eq!(secondary["intent"]["provider"], Value::Null);
    assert_eq!(secondary["intent"]["region"], Value::Null);

    // 2. Declare the shared data source both clients' `primary` will land
    // on.
    let response = send(&plane.router, get(DATA_SOURCES_PATH)).await;
    let empty_data_sources_tag = entity_tag(&response);
    let response = send(
        &plane.router,
        put_data_source(
            &format!("{DATA_SOURCES_PATH}/shared-postgres-nz-01"),
            &format!("\"{empty_data_sources_tag}\""),
            &shared_declaration(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut data_sources_tag = entity_tag(&response);

    // 3. The same intent is now placeable: nothing recorded, and nothing
    // refuses it -- `placed` and `refusal` are both `null`. Reading it
    // moved nothing.
    let response = send(&plane.router, get("/api/clients/acme/placements")).await;
    assert_eq!(
        entity_tag(&response),
        initial_placements_tag,
        "reading placements must not move their revision"
    );
    let body = body_of(response).await;
    let primary = row(&body, "primary");
    assert_eq!(primary["placed"], Value::Null);
    assert_eq!(primary["refusal"], Value::Null);

    // 4. Placing it records the tenant id as the discriminator value.
    let response = send(
        &plane.router,
        post_place(
            "/api/clients/acme/placements/primary",
            &format!("\"{initial_placements_tag}\""),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut placements_tag = entity_tag(&response);
    let body = body_of(response).await;
    let primary = row(&body, "primary");
    assert_eq!(primary["placed"]["dataSource"], "shared-postgres-nz-01");
    assert_eq!(primary["placed"]["isolation"]["kind"], "discriminator");
    assert_eq!(primary["placed"]["isolation"]["column"], "tenant_key");
    assert_eq!(primary["placed"]["isolation"]["value"], "acme");
    assert!(primary["placed"]["placedAt"].is_string(), "{body}");
    assert_eq!(primary["refusal"], Value::Null);

    // 5. A second client on the same shared source gets its own
    // discriminator value.
    let response = send(
        &plane.router,
        post_place(
            "/api/clients/globex/placements/primary",
            &format!("\"{placements_tag}\""),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    placements_tag = entity_tag(&response);
    let body = body_of(response).await;
    let primary = row(&body, "primary");
    assert_eq!(primary["placed"]["dataSource"], "shared-postgres-nz-01");
    assert_eq!(primary["placed"]["isolation"]["value"], "globex");

    // 6. Declare a dedicated data source for `audit`.
    let response = send(
        &plane.router,
        put_data_source(
            &format!("{DATA_SOURCES_PATH}/dedicated-postgres-01"),
            &format!("\"{data_sources_tag}\""),
            &dedicated_declaration(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    data_sources_tag = entity_tag(&response);

    // 7. Acme places on it -- a dedicated source is one tenant's.
    let response = send(
        &plane.router,
        post_place(
            "/api/clients/acme/placements/audit",
            &format!("\"{placements_tag}\""),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    placements_tag = entity_tag(&response);
    let body = body_of(response).await;
    let audit = row(&body, "audit");
    assert_eq!(audit["placed"]["dataSource"], "dedicated-postgres-01");
    assert_eq!(audit["placed"]["isolation"], json!({"kind": "database"}));

    // 8. Globex asks for the same dedicated source and is refused --
    // already somebody else's. The message is `AllMatchingSourcesOccupied`'s
    // (N2), not `NoDataSourceAdmits`'s: a dedicated source matching the
    // class exists, it is just already a tenant's, and an operator told to
    // "declare" one here would go looking for a mistake that is not
    // theirs.
    let response = send(
        &plane.router,
        post_place(
            "/api/clients/globex/placements/audit",
            &format!("\"{placements_tag}\""),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "placement_refused");
    assert_eq!(
        body["error"]["message"], "every dedicated data source that matches is already somebody's tenant",
        "{body}"
    );

    // 9. A stale `If-Match` is refused rather than silently applied over
    // every placement recorded since it was read -- checked before the
    // selector ever runs, so even a request for an already-placed logical
    // answers the conflict rather than `AlreadyPlaced`.
    let response = send(
        &plane.router,
        post_place(
            "/api/clients/acme/placements/primary",
            &format!("\"{initial_placements_tag}\""),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "revision_conflict");

    // 10. Removing the shared source is refused while both tenants are
    // still placed on it, naming both.
    let response = send(
        &plane.router,
        delete_data_source(
            &format!("{DATA_SOURCES_PATH}/shared-postgres-nz-01"),
            &format!("\"{data_sources_tag}\""),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "data_source_in_use");
    let message = body["error"]["message"]
        .as_str()
        .expect("a message must be a string");
    assert!(message.contains("acme"), "{message}");
    assert!(message.contains("globex"), "{message}");

    // 11. Declaring and then removing a source nothing ever asked for
    // succeeds, and the list that follows no longer carries it.
    let response = send(
        &plane.router,
        put_data_source(
            &format!("{DATA_SOURCES_PATH}/unused-postgres-01"),
            &format!("\"{data_sources_tag}\""),
            &unused_declaration(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    data_sources_tag = entity_tag(&response);

    let response = send(
        &plane.router,
        delete_data_source(
            &format!("{DATA_SOURCES_PATH}/unused-postgres-01"),
            &format!("\"{data_sources_tag}\""),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_of(response).await;
    assert!(
        body["dataSources"]
            .as_array()
            .expect("dataSources must be an array")
            .iter()
            .all(|item| item["id"] != "unused-postgres-01"),
        "{body}"
    );

    // 12. A logical data source acme's document never named is refused --
    // 422 `placement_refused`, the same code the selector's own refusals
    // carry, naming what was asked for.
    let response = send(
        &plane.router,
        post_place(
            "/api/clients/acme/placements/nonexistent",
            &format!("\"{placements_tag}\""),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "placement_refused");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("a message must be a string")
            .contains("nonexistent"),
        "{body}"
    );

    // 13. A `{logical}` that does not even parse is a `400`, before
    // anything is read.
    let response = send(
        &plane.router,
        post_place(
            "/api/clients/acme/placements/9-not-a-valid-name",
            &format!("\"{placements_tag}\""),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "invalid_request");

    // 14. POST with no `If-Match` at all is refused before anything is
    // read -- `428`, naming the missing precondition rather than guessing
    // which revision was meant.
    let response = send(
        &plane.router,
        as_operator("POST", "/api/clients/acme/placements/primary")
            .body(Body::empty())
            .expect("the request must build"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "revision_required");

    // 15. DELETE with no `If-Match` is refused the same way.
    let response = send(
        &plane.router,
        as_operator("DELETE", &format!("{DATA_SOURCES_PATH}/dedicated-postgres-01"))
            .body(Body::empty())
            .expect("the request must build"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "revision_required");

    // 16. GET for a client nobody ever created is a `404` `unknown_client`,
    // not an empty or a generic error -- the same code every other
    // client-scoped route answers for one.
    let response = send(&plane.router, get("/api/clients/no-such-client/placements")).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "unknown_client");
}
