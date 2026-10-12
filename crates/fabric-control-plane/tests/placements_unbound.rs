//! The catalogue resources a client's tenant cannot reach, on
//! `/api/clients/{clientId}/placements`, through the real router (G8, ADR
//! 0023 "A resource on a logical data source a client never asked for").
//!
//! The runtime serves one catalogue to every tenant, and a resource on a
//! logical data source a tenant does not bind answers `UnboundDataSource`
//! (500) to that tenant (ADR 0018 part 4). The publisher refuses only an
//! entry that no tenant binds, so these pairs are published; the Data tab
//! is where an operator sees them.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines
)]

mod support;

use axum::body::Body;
use axum::Router;
use fabric_client_model::ClientDocument;
use fabric_control_plane::{ChangeContext, ClientRepository as _};
use http::{Request, StatusCode};
use serde_json::{json, Value};
use support::platform_fixture::platform_binding;
use support::{as_operator, control_plane_with_platform, entity_tag, json as body_of, send};

/// "acme", asking for a shared `primary` and a dedicated `audit`, and
/// nothing for `reference`.
const ACME: &str = r"
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
      region: nz
    audit:
      class: dedicated
";

fn get(path: &str) -> Request<Body> {
    as_operator("GET", path).body(Body::empty()).unwrap()
}

async fn catalogue_command(router: &Router, revision: &Value, command: &Value) -> Value {
    let builder = as_operator("POST", "/api/catalogue").header("content-type", "application/json");
    let builder = match revision.as_str() {
        Some(revision) => builder.header("if-match", format!("\"{revision}\"")),
        None => builder.header("if-none-match", "*"),
    };
    let response = send(router, builder.body(Body::from(command.to_string())).unwrap()).await;
    let status = response.status();
    let body = body_of(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

fn resource(name: &str, data_source: &str) -> Value {
    json!({"name": name, "dataSource": data_source, "collection": name})
}

/// Publishes release 1 of `id` with `resources`, starting from `revision`,
/// and returns the catalogue's new revision.
async fn publish(router: &Router, revision: Value, id: &str, resources: &Value) -> Value {
    let created = catalogue_command(
        router,
        &revision,
        &json!({"action": "createApplication", "id": id, "name": id}),
    )
    .await;
    let definition = json!({"name": id, "description": "", "domain": "{client}.example.com",
        "components": [], "features": [],
        "plans": [{"id": "standard", "name": "Standard", "description": "", "features": [], "configuration": {}}],
        "fields": [], "navigation": [], "resources": resources});
    let saved = catalogue_command(
        router,
        &created["revision"],
        &json!({"action": "saveApplication", "id": id, "definition": definition}),
    )
    .await;
    let published = catalogue_command(
        router,
        &saved["revision"],
        &json!({"action": "publishApplication", "id": id, "note": "first"}),
    )
    .await;
    published["revision"].clone()
}

/// `(logical, declared, [resource@application:version])` for every unbound
/// entry, in the order the body lists them.
fn unbound(body: &Value) -> Vec<(String, bool, Vec<String>)> {
    assert_eq!(body["unbound"]["state"], "checked", "{body}");
    body["unbound"]["dataSources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["logical"].as_str().unwrap().to_owned(),
                entry["declared"].as_bool().unwrap(),
                entry["resources"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|resource| {
                        format!(
                            "{}@{}:{}",
                            resource["resource"].as_str().unwrap(),
                            resource["application"].as_str().unwrap(),
                            resource["version"]
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

fn entry(logical: &str, declared: bool, resources: &[&str]) -> (String, bool, Vec<String>) {
    (
        logical.to_owned(),
        declared,
        resources.iter().map(|resource| (*resource).to_owned()).collect(),
    )
}

#[tokio::test]
async fn every_resource_on_a_logical_data_source_the_client_does_not_bind_is_listed_until_it_is_placed() {
    let (platform, _fake) = platform_binding().await;
    let plane = control_plane_with_platform(platform);
    plane
        .repository
        .insert(&ClientDocument::parse(ACME).unwrap())
        .unwrap();

    // 1. Nothing published: the check is made, and nothing is unbound.
    let body = body_of(send(&plane.router, get("/api/clients/acme/placements")).await).await;
    assert_eq!(unbound(&body), []);

    // 2. Two applications publish resources on `primary`, `audit` and
    // `reference`. Nothing of acme's is placed, so every one is unbound, and
    // only `reference` is one its document does not ask for.
    let revision = publish(
        &plane.router,
        Value::Null,
        "workspec",
        &json!([
            resource("customers", "primary"),
            resource("audit-events", "audit")
        ]),
    )
    .await;
    publish(
        &plane.router,
        revision,
        "ledger",
        &json!([resource("journals", "audit"), resource("rates", "reference")]),
    )
    .await;

    let response = send(&plane.router, get("/api/clients/acme/placements")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let placements_tag = entity_tag(&response);
    let body = body_of(response).await;
    assert_eq!(
        unbound(&body),
        [
            entry("audit", true, &["audit-events@workspec:1", "journals@ledger:1"]),
            entry("primary", true, &["customers@workspec:1"]),
            entry("reference", false, &["rates@ledger:1"]),
        ]
    );

    // 3. Placing `primary` binds it: the write's own response no longer
    // lists it, and a following read agrees.
    let response = send(&plane.router, get("/api/platform/data-sources")).await;
    let data_sources_tag = entity_tag(&response);
    let declared = as_operator("PUT", "/api/platform/data-sources/shared-postgres-nz-01")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header("If-Match", format!("\"{data_sources_tag}\""))
        .body(Body::from(
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
            .to_string(),
        ))
        .unwrap();
    assert_eq!(send(&plane.router, declared).await.status(), StatusCode::OK);

    let place_primary = as_operator("POST", "/api/clients/acme/placements/primary")
        .header("If-Match", format!("\"{placements_tag}\""))
        .body(Body::empty())
        .unwrap();
    let response = send(&plane.router, place_primary).await;
    assert_eq!(response.status(), StatusCode::OK);
    let placed_tag = entity_tag(&response);
    let expected = [
        entry("audit", true, &["audit-events@workspec:1", "journals@ledger:1"]),
        entry("reference", false, &["rates@ledger:1"]),
    ];
    assert_eq!(unbound(&body_of(response).await), expected);

    let response = send(&plane.router, get("/api/clients/acme/placements")).await;
    assert_eq!(
        entity_tag(&response),
        placed_tag,
        "reading must not move the revision"
    );
    assert_eq!(unbound(&body_of(response).await), expected);
}

#[tokio::test]
async fn a_catalogue_that_cannot_be_derived_leaves_the_placements_readable_and_says_why_nothing_was_checked()
{
    let (platform, _fake) = platform_binding().await;
    let plane = control_plane_with_platform(platform);
    plane
        .repository
        .insert(&ClientDocument::parse(ACME).unwrap())
        .unwrap();
    publish(
        &plane.router,
        Value::Null,
        "workspec",
        &json!([resource("customers", "primary")]),
    )
    .await;

    // A hand edit gives a second application the same resource name, which
    // release publication refuses and the derived catalogue reports.
    let stored = plane.repository.catalogue().await.unwrap();
    let mut edited = stored.catalogue.clone();
    let mut other = edited.applications[0].clone();
    other.id = fabric_client_model::ClientId::try_new("other").unwrap();
    edited.applications.push(other);
    plane
        .repository
        .save_catalogue(
            &edited,
            stored.revision.as_ref(),
            &ChangeContext {
                requested_by: "someone-in-git".to_owned(),
                summary: "hand edit".to_owned(),
            },
        )
        .await
        .unwrap();

    let response = send(&plane.router, get("/api/clients/acme/placements")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_of(response).await;
    assert_eq!(body["placements"].as_array().unwrap().len(), 2, "{body}");
    assert_eq!(body["unbound"]["state"], "unchecked", "{body}");
    assert_eq!(
        body["unbound"]["reason"],
        "resource 'customers' is declared by both 'workspec' and 'other'"
    );
}
