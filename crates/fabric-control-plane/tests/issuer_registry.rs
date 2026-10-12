//! The issuer registry (D01-4) through the real router, the controller's
//! pass and a real filesystem target: a client is registered by its realm's
//! issuer only while its lifecycle permits routing, judged against the
//! tenants document the same pass publishes; the generation advances only
//! when the registrations change; a tombstoned client leaves the registry
//! while its tenant binding stays published (withdrawal is deferred); and
//! the platform panel reports the three readiness gates.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::indexing_slicing
)]

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::Router;
use fabric_client_model::{ClientDocument, ClientId, ClientRevision};
use fabric_control_plane::PublicationSink;
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_platform_management::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, DataSourceDeclaration, DesiredRevision, IsolationModelDocument,
    PlacementClassDocument, PlacementRecord, PoolSettingsDocument,
};
use fabric_reconciliation::ReconciliationOutcome;
use fabric_runtime_publication::{FilesystemRuntimePublication, IssuerOrigin};
use http::StatusCode;
use serde_json::{json, Value};
use support::platform_fixture::platform_binding;
use support::{
    as_operator, control_plane_with_publication, json as body_of, send, TempDir, TestControlPlane,
};

const DATA_SOURCE: &str = "shared-postgres-nz-01";

fn client_document(name: &str) -> ClientDocument {
    ClientDocument::parse(&format!(
        r"
apiVersion: fabric.fieldstate.nz/v2
kind: Client
metadata:
  name: {name}
spec:
  displayName: {name}
  identity:
    realm: {name}
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients: []
  data:
    primary:
      class: shared
"
    ))
    .unwrap()
}

fn sink(dir: &TempDir) -> PublicationSink {
    PublicationSink {
        target: Arc::new(FilesystemRuntimePublication::new(
            dir.path().join("tenants.json"),
            dir.path().join("data-sources.json"),
            dir.path().join("catalog.json"),
        )),
        issuer_origin: Some(IssuerOrigin::parse("https://identity.example.invalid").unwrap()),
    }
}

fn declared_data_source() -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(DATA_SOURCE).unwrap(),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres-nz").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").unwrap(),
        },
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: Some("NZ".to_owned()),
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument {
            writable: true,
            accepts_new_tenants: true,
        },
        discriminator: Some(fabric_platform_management::Discriminator {
            column: fabric_platform_management::FieldName::try_new("tenant_key").unwrap(),
        }),
        labels: BTreeMap::new(),
    }
}

fn placement(tenant: &str) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(tenant).unwrap(),
        logical: LogicalDataSourceName::try_new("primary").unwrap(),
        revision: BindingRevision::new(1),
        data_source: DataSourceId::try_new(DATA_SOURCE).unwrap(),
        isolation: IsolationModelDocument::Discriminator {
            column: fabric_platform_management::FieldName::try_new("tenant_key").unwrap(),
            value: format!("{tenant}-key"),
        },
        placed_at: "2026-10-11T00:00:00Z".to_owned(),
    }
}

async fn publish_one_application_with_a_resource(router: &Router) {
    async fn command(router: &Router, revision: Option<&str>, value: &Value) -> Value {
        let builder = as_operator("POST", "/api/catalogue").header("content-type", "application/json");
        let builder = match revision {
            Some(revision) => builder.header("if-match", format!("\"{revision}\"")),
            None => builder.header("if-none-match", "*"),
        };
        let response = send(router, builder.body(Body::from(value.to_string())).unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        body_of(response).await
    }

    let created = command(
        router,
        None,
        &json!({"action":"createApplication","id":"workspec","name":"Workspec"}),
    )
    .await;
    let definition = json!({
        "name": "Workspec", "description": "", "domain": "{client}.example.com",
        "components": [], "features": [],
        "plans": [{"id":"standard","name":"Standard","description":"","features":[],"configuration":{}}],
        "fields": [], "navigation": [],
        "resources": [{
            "name": "customers", "dataSource": "primary", "collection": "customers", "keyField": "id",
            "operations": ["read", "list"], "queryableFields": [],
        }],
    });
    let saved = command(
        router,
        created["revision"].as_str(),
        &json!({"action":"saveApplication","id":"workspec","definition":definition}),
    )
    .await;
    command(
        router,
        saved["revision"].as_str(),
        &json!({"action":"publishApplication","id":"workspec","note":"v1"}),
    )
    .await;
}

async fn publish(router: &Router) -> Value {
    let request = as_operator("POST", "/api/platform/publication")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let response = send(router, request).await;
    assert_eq!(response.status(), StatusCode::OK);
    body_of(response).await
}

fn applied(plane: &TestControlPlane, client: &str, revision: &ClientRevision) {
    plane.statuses.record(
        &ClientId::try_new(client).unwrap(),
        revision.clone(),
        &ReconciliationOutcome::converged(),
        1,
    );
}

fn registry(dir: &TempDir) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.path().join("issuer-registry.json")).unwrap()).unwrap()
}

fn registered(dir: &TempDir) -> Vec<(String, String)> {
    registry(dir)["issuers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["issuer"].as_str().unwrap().to_owned(),
                entry["tenant"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn tenants_published(dir: &TempDir) -> Vec<String> {
    let tenants: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("tenants.json")).unwrap()).unwrap();
    tenants
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["tenant"].as_str().unwrap().to_owned())
        .collect()
}

fn issuer(realm: &str) -> String {
    format!("https://identity.example.invalid/realms/{realm}")
}

#[tokio::test]
async fn only_routable_clients_are_registered_and_each_change_is_one_new_generation() {
    let dir = TempDir::new("issuer-registry");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    let globex = plane.repository.insert(&client_document("globex")).unwrap();
    let initech = plane.repository.insert(&client_document("initech")).unwrap();
    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(
        DesiredRevision::new("pl-1"),
        vec![placement("acme"), placement("globex"), placement("initech")],
    );
    publish_one_application_with_a_resource(&plane.router).await;

    // globex's identity is applied; it is judged against the tenants
    // document this same pass publishes, so it is registered on the first
    // pass rather than the one after. acme declares no data and initech's
    // identity is not applied: both are bound and neither is routable.
    applied(&plane, "globex", &globex);
    let first = publish(&plane.router).await;
    assert_eq!(tenants_published(&dir), ["acme", "globex", "initech"]);
    assert_eq!(registered(&dir), [(issuer("globex"), "globex".to_owned())]);
    assert_eq!(registry(&dir)["generation"], 1);
    assert_eq!(first["documents"]["registry"], 1, "{first}");
    assert_eq!(first["readiness"]["identity"]["met"], true, "{first}");
    let excluded: Vec<&str> = first["readiness"]["excludedFromRegistry"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["tenant"].as_str().unwrap())
        .collect();
    assert_eq!(excluded, ["acme", "initech"], "{first}");
    assert_eq!(first["readiness"]["excludedFromRegistry"][1]["kind"], "notReady");
    assert!(first["readiness"]["excludedFromRegistry"][1]["detail"]
        .as_str()
        .unwrap()
        .contains("identity: identity has not been applied at this revision yet"));
    assert_eq!(
        first["readiness"]["registryApplication"]["state"], "unobserved",
        "{first}"
    );

    let unchanged = publish(&plane.router).await;
    assert_eq!(unchanged["documents"]["registry"], 1, "{unchanged}");
    assert_eq!(registry(&dir)["generation"], 1);

    applied(&plane, "initech", &initech);
    publish(&plane.router).await;
    assert_eq!(
        registered(&dir),
        [
            (issuer("globex"), "globex".to_owned()),
            (issuer("initech"), "initech".to_owned()),
        ]
    );
    assert_eq!(registry(&dir)["generation"], 2);

    // Tombstoning globex refuses routing: its issuer leaves the registry in
    // a new generation, and its tenant binding stays published, because
    // withdrawal is deferred. The lifecycle does not claim withdrawal.
    let response = send(
        &plane.router,
        as_operator("POST", "/api/clients/globex/tombstone")
            .header("content-type", "application/json")
            .header("if-match", format!("\"{}\"", globex.as_str()))
            .body(Body::from(json!({"reason": "Contract ended"}).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let retired = publish(&plane.router).await;
    assert_eq!(registered(&dir), [(issuer("initech"), "initech".to_owned())]);
    assert_eq!(registry(&dir)["generation"], 3);
    assert_eq!(tenants_published(&dir), ["acme", "globex", "initech"]);
    let globex_exclusion = retired["readiness"]["excludedFromRegistry"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["tenant"] == "globex")
        .cloned()
        .unwrap();
    assert_eq!(globex_exclusion["kind"], "retired", "{globex_exclusion}");
    assert!(globex_exclusion["detail"]
        .as_str()
        .unwrap()
        .contains("tombstoned"));
    let lifecycle = send(
        &plane.router,
        as_operator("GET", "/api/clients/globex")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let lifecycle = body_of(lifecycle).await;
    assert_eq!(
        lifecycle["lifecycle"]["withdrawal"]["state"], "pending",
        "{lifecycle}"
    );

    let record: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("publication.json")).unwrap()).unwrap();
    assert_eq!(record["registry"]["generation"], 3, "{record}");
}

#[tokio::test]
async fn the_platform_panel_reports_the_three_gates_and_never_meets_connectors_itself() {
    let dir = TempDir::new("issuer-registry-gates");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));

    let response = send(
        &plane.router,
        as_operator("GET", "/api/platform").body(Body::empty()).unwrap(),
    )
    .await;
    let before = body_of(response).await;
    for gate in ["documents", "identity", "connectors"] {
        assert_eq!(before["publication"]["readiness"][gate]["met"], false, "{before}");
    }

    let globex = plane.repository.insert(&client_document("globex")).unwrap();
    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(DesiredRevision::new("pl-1"), vec![placement("globex")]);
    publish_one_application_with_a_resource(&plane.router).await;
    applied(&plane, "globex", &globex);
    publish(&plane.router).await;

    let response = send(
        &plane.router,
        as_operator("GET", "/api/platform").body(Body::empty()).unwrap(),
    )
    .await;
    let after = body_of(response).await;
    let readiness = &after["publication"]["readiness"];
    assert_eq!(readiness["documents"]["met"], true, "{after}");
    assert_eq!(readiness["identity"]["met"], true, "{after}");
    assert_eq!(readiness["connectors"]["met"], false, "{after}");
    assert!(readiness["connectors"]["reasons"][0]
        .as_str()
        .unwrap()
        .contains("postgres-nz"));
}

async fn edit_display_name(
    plane: &TestControlPlane,
    client: &str,
    revision: &ClientRevision,
) -> ClientRevision {
    let response = send(
        &plane.router,
        as_operator("PUT", &format!("/api/clients/{client}/product"))
            .header("content-type", "application/json")
            .header("if-match", format!("\"{}\"", revision.as_str()))
            .body(Body::from(
                json!({
                    "displayName": "Renamed", "hosts": [], "legalName": "Renamed Ltd",
                    "region": "NZ", "timezone": "Pacific/Auckland", "configuration": {}, "applications": []
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = body_of(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    ClientRevision::try_new(body["client"]["revision"].as_str().unwrap()).unwrap()
}

async fn ready_plane(dir: &TempDir) -> (TestControlPlane, ClientRevision) {
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(dir));
    let globex = plane.repository.insert(&client_document("globex")).unwrap();
    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(DesiredRevision::new("pl-1"), vec![placement("globex")]);
    publish_one_application_with_a_resource(&plane.router).await;
    (plane, globex)
}

/// Option (a) on #108: identity evidence pending reconciliation excludes the
/// client, fail closed, after a client write and after a control-plane
/// restart alike; reconciliation brings the same issuer back (the realm is
/// unchanged), and repeating an unchanged state never stamps a generation.
#[tokio::test]
async fn pending_identity_after_a_write_or_a_restart_excludes_then_reconciliation_restores_without_churn() {
    let dir = TempDir::new("issuer-registry-pending");
    let globex_only = || vec![(issuer("globex"), "globex".to_owned())];
    let mut generations = Vec::new();
    let mut publish_twice = |dir: &TempDir, first: Value, second: Value| {
        assert_eq!(
            first["documents"]["registry"], second["documents"]["registry"],
            "{second}"
        );
        generations.push(registry(dir)["generation"].as_u64().unwrap());
        first
    };

    let (plane, globex) = ready_plane(&dir).await;
    applied(&plane, "globex", &globex);
    publish_twice(&dir, publish(&plane.router).await, publish(&plane.router).await);
    assert_eq!(registered(&dir), globex_only());

    // A client write: identity is pending at the new revision.
    let edited = edit_display_name(&plane, "globex", &globex).await;
    let pending = publish_twice(&dir, publish(&plane.router).await, publish(&plane.router).await);
    assert_eq!(registered(&dir), []);
    let exclusion = &pending["readiness"]["excludedFromRegistry"][0];
    assert_eq!(exclusion["kind"], "notReady", "{pending}");
    assert!(exclusion["detail"]
        .as_str()
        .unwrap()
        .contains("identity: identity has not been applied at this revision yet"));
    assert_eq!(pending["readiness"]["registryApplication"]["state"], "unobserved");
    assert_eq!(tenants_published(&dir), ["globex"], "the binding is untouched");

    applied(&plane, "globex", &edited);
    publish_twice(&dir, publish(&plane.router).await, publish(&plane.router).await);
    assert_eq!(registered(&dir), globex_only());

    // A restart: a new control plane over the same published state, its
    // reconciliation status empty, so identity is pending again.
    drop(plane);
    let (restarted, globex) = ready_plane(&dir).await;
    publish_twice(
        &dir,
        publish(&restarted.router).await,
        publish(&restarted.router).await,
    );
    assert_eq!(registered(&dir), []);

    applied(&restarted, "globex", &globex);
    publish_twice(
        &dir,
        publish(&restarted.router).await,
        publish(&restarted.router).await,
    );
    assert_eq!(registered(&dir), globex_only());

    assert_eq!(generations, [1, 2, 3, 4, 5]);
}

/// D01-19b: a client whose projected identity client was withheld, or whose
/// stored projection cannot be computed even with nothing to withhold, is
/// not ready and is left out of the registry. Its tenant binding stays
/// published: exclusion is from what Fabric publishes, not a revocation.
#[tokio::test]
async fn a_withheld_or_unverifiable_projection_is_not_registered_and_its_binding_stays_published() {
    let dir = TempDir::new("issuer-registry-projection");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    let globex = plane.repository.insert(&client_document("globex")).unwrap();
    let initech = plane.repository.insert(&client_document("initech")).unwrap();
    let unreadable = ClientDocument::parse(&format!(
        "{}  product:\n    region: [nz]\n",
        client_document("umbrella").render().unwrap()
    ))
    .unwrap();
    assert!(unreadable.product().is_err());
    let umbrella = plane.repository.insert(&unreadable).unwrap();
    fake.seed(DesiredRevision::new("ds-1"), vec![declared_data_source()]);
    fake.seed_placements(
        DesiredRevision::new("pl-1"),
        vec![placement("globex"), placement("initech"), placement("umbrella")],
    );
    publish_one_application_with_a_resource(&plane.router).await;

    applied(&plane, "globex", &globex);
    plane.statuses.record(
        &ClientId::try_new("initech").unwrap(),
        initech,
        &ReconciliationOutcome::converged()
            .withholding(vec![
                fabric_client_model::OidcClientId::try_new("analytics").unwrap()
            ]),
        1,
    );
    applied(&plane, "umbrella", &umbrella);
    let pass = publish(&plane.router).await;

    assert_eq!(registered(&dir), [(issuer("globex"), "globex".to_owned())]);
    let excluded: BTreeMap<String, String> = pass["readiness"]["excludedFromRegistry"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["tenant"].as_str().unwrap().to_owned(),
                entry["detail"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert!(excluded["initech"].contains("was withheld"), "{pass}");
    assert!(
        excluded["umbrella"].contains("cannot be checked against their projection"),
        "{pass}"
    );
    assert_eq!(
        tenants_published(&dir),
        ["globex", "initech", "umbrella"],
        "a tenant binding is not withdrawn by this"
    );
}
