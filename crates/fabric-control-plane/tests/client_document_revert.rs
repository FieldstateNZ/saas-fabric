//! O5: what a Git revert of a client document does through the real router,
//! the controller's pass and a real filesystem publication target (D01-27:
//! restoring a client must keep conditional writes and audit evidence, and
//! any break-glass Git effect is characterized here before it is claimed as
//! a supported rollback).
//!
//! A revert is stood in for by writing the document as it stood before the
//! reverted write straight to the repository, beside the console, or, for a
//! revert of the commit that created a client, by removing it. That is what
//! `clients/<id>/client.yaml` holds after `git revert`, and it reaches the
//! control plane as an ordinary read: no console rule is applied to it, and
//! nothing is recorded in the audit trail.
//!
//! The in-memory repository's revisions are a write counter, so here a
//! revert is always a new revision. The Git adapter names a revision by its
//! blob, so a revert to identical bytes brings the earlier one back; that is
//! characterized in `fabric-client-git`'s `desired_state_repository.rs`.

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
use fabric_control_plane::{
    AuditStream, AuditTrail as _, ChangeContext, ClientRepository as _, PublicationSink,
};
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_platform_management::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, DataSourceDeclaration, DesiredRevision, IsolationModelDocument,
    PlacementClassDocument, PlacementRecord, PoolSettingsDocument,
};
use fabric_reconciliation::ReconciliationOutcome;
use fabric_runtime_publication::{FilesystemRuntimePublication, IssuerOrigin};
use http::{header, StatusCode};
use serde_json::{json, Value};
use support::platform_fixture::platform_binding;
use support::{
    as_operator, control_plane_with_publication, json as body_of, send, TempDir, TestControlPlane,
};

const DATA_SOURCE: &str = "shared-postgres-nz-01";
const DOCUMENTS: [&str; 5] = [
    "tenants.json",
    "data-sources.json",
    "catalog.json",
    "issuer-registry.json",
    "publication.json",
];

fn globex() -> ClientId {
    ClientId::try_new("globex").unwrap()
}

fn globex_document() -> ClientDocument {
    ClientDocument::parse(
        r"
apiVersion: fabric.fieldstate.nz/v2
kind: Client
metadata:
  name: globex
spec:
  displayName: globex
  identity:
    realm: globex
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients: []
  data:
    primary:
      class: shared
",
    )
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

fn data_source() -> DataSourceDeclaration {
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

fn placement() -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new("globex").unwrap(),
        logical: LogicalDataSourceName::try_new("primary").unwrap(),
        revision: BindingRevision::new(1),
        data_source: DataSourceId::try_new(DATA_SOURCE).unwrap(),
        isolation: IsolationModelDocument::Discriminator {
            column: fabric_platform_management::FieldName::try_new("tenant_key").unwrap(),
            value: "globex-key".to_owned(),
        },
        placed_at: "2026-10-12T00:00:00Z".to_owned(),
    }
}

/// Sends one catalogue command at `revision` (`None` for the first write)
/// and returns the new revision.
async fn catalogue_command(router: &Router, revision: Option<&str>, body: &Value) -> Option<String> {
    let builder = as_operator("POST", "/api/catalogue").header(header::CONTENT_TYPE, "application/json");
    let builder = match revision {
        Some(revision) => builder.header(header::IF_MATCH, format!("\"{revision}\"")),
        None => builder.header(header::IF_NONE_MATCH, "*"),
    };
    let response = send(router, builder.body(Body::from(body.to_string())).unwrap()).await;
    let status = response.status();
    let body = body_of(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["revision"].as_str().map(str::to_owned)
}

/// `workspec` released once, with `customers` on `primary`.
async fn release_workspec(router: &Router) {
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
    let revision = catalogue_command(
        router,
        None,
        &json!({"action":"createApplication","id":"workspec","name":"Workspec"}),
    )
    .await;
    let revision = catalogue_command(
        router,
        revision.as_deref(),
        &json!({"action":"saveApplication","id":"workspec","definition":definition}),
    )
    .await;
    catalogue_command(
        router,
        revision.as_deref(),
        &json!({"action":"publishApplication","id":"workspec","note":"1"}),
    )
    .await;
}

/// Runs one pass through `POST /api/platform/publication` and returns the
/// whole response.
async fn publish(router: &Router) -> Value {
    let response = send(
        router,
        as_operator("POST", "/api/platform/publication")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    body_of(response).await
}

async fn get(router: &Router, path: &str) -> (StatusCode, Value) {
    let response = send(router, as_operator("GET", path).body(Body::empty()).unwrap()).await;
    let status = response.status();
    (status, body_of(response).await)
}

fn applied(plane: &TestControlPlane, revision: &ClientRevision) {
    plane.statuses.record(
        &globex(),
        revision.clone(),
        &ReconciliationOutcome::converged(),
        1,
    );
}

fn read(dir: &TempDir, name: &str) -> Vec<u8> {
    std::fs::read(dir.path().join(name)).unwrap()
}

fn document(dir: &TempDir, name: &str) -> Value {
    serde_json::from_slice(&read(dir, name)).unwrap()
}

fn published(dir: &TempDir) -> Vec<Vec<u8>> {
    DOCUMENTS.iter().map(|name| read(dir, name)).collect()
}

fn registered(dir: &TempDir) -> Vec<String> {
    document(dir, "issuer-registry.json")["issuers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["tenant"].as_str().unwrap().to_owned())
        .collect()
}

fn generation(dir: &TempDir) -> u64 {
    document(dir, "issuer-registry.json")["generation"]
        .as_u64()
        .unwrap()
}

fn tenants(dir: &TempDir) -> Vec<String> {
    document(dir, "tenants.json")
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["tenant"].as_str().unwrap().to_owned())
        .collect()
}

/// The exclusion kind the pass reported for `globex`, if any.
fn exclusion(body: &Value) -> Option<String> {
    body["readiness"]["excludedFromRegistry"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["tenant"] == "globex")
        .map(|entry| entry["kind"].as_str().unwrap().to_owned())
}

fn git_revert(summary: &str) -> ChangeContext {
    ChangeContext {
        requested_by: "someone-in-git".to_owned(),
        summary: format!("Revert \"{summary}\""),
    }
}

/// Writes `document` over `client` as one commit would, against whatever
/// revision is current.
async fn commit(
    plane: &TestControlPlane,
    client: &ClientId,
    document: &ClientDocument,
    summary: &str,
) -> ClientRevision {
    let current = plane.repository.get(client).await.unwrap();
    plane
        .repository
        .update(client, document, &current.revision, &git_revert(summary))
        .await
        .unwrap()
}

async fn audited(plane: &TestControlPlane, client: &ClientId) -> Vec<String> {
    plane
        .audit
        .read(&AuditStream::Client(client.clone()))
        .await
        .unwrap()
        .entries
        .iter()
        .map(|entry| entry.record().event().to_owned())
        .collect()
}

/// `globex` placed on `primary`, its identity applied at the revision it
/// was stored at, and a pass that has published its tenant binding and
/// registered its issuer.
async fn registered_globex(label: &str) -> (TempDir, TestControlPlane, ClientRevision) {
    let dir = TempDir::new(label);
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    let revision = plane.repository.insert(&globex_document()).unwrap();
    fake.seed(DesiredRevision::new("data-sources-1"), vec![data_source()]);
    fake.seed_placements(DesiredRevision::new("placements-1"), vec![placement()]);
    release_workspec(&plane.router).await;
    applied(&plane, &revision);
    publish(&plane.router).await;
    assert_eq!(registered(&dir), ["globex"]);
    assert_eq!(generation(&dir), 1);
    (dir, plane, revision)
}

async fn tombstone(router: &Router, revision: &ClientRevision) -> ClientRevision {
    let response = send(
        router,
        as_operator("POST", "/api/clients/globex/tombstone")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, format!("\"{}\"", revision.as_str()))
            .body(Body::from(json!({"reason": "Contract ended"}).to_string()))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = body_of(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    ClientRevision::try_new(body["revision"].as_str().unwrap()).unwrap()
}

/// Characterizes D01-14's tombstone undone outside the console: nothing
/// refuses it, nothing audits it, and the issuer is registered again once
/// identity is applied at the reverted revision.
#[tokio::test]
async fn reverting_a_tombstone_reinstates_the_client_and_its_issuer_with_no_audit_record_of_it() {
    let (dir, plane, ready) = registered_globex("client-revert-tombstone").await;
    let router = &plane.router;
    let before_tombstone = plane.repository.get(&globex()).await.unwrap().document;

    tombstone(router, &ready).await;
    let retired = publish(router).await;
    assert_eq!(exclusion(&retired).as_deref(), Some("retired"), "{retired}");
    assert!(registered(&dir).is_empty());
    assert_eq!(generation(&dir), 2);
    let tombstoned_events = |activity: &Value| {
        activity["activity"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["action"] == "Client tombstoned")
            .count()
    };
    assert_eq!(tombstoned_events(&get(router, "/api/activity").await.1), 1);
    let audit_before = audited(&plane, &globex()).await;
    assert_eq!(audit_before, ["control_plane.audit.client_tombstoned"]);

    let reverted = commit(&plane, &globex(), &before_tombstone, "tombstone globex").await;

    // The console reads the reverted document as it is: the tombstone is
    // gone, and so is the activity entry that recorded it. The client is
    // incomplete only because identity is not applied at the new revision.
    let (status, client) = get(router, "/api/clients/globex").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(client["lifecycle"]["state"], "incomplete", "{client}");
    assert_eq!(client["lifecycle"]["tombstone"], Value::Null, "{client}");
    assert_eq!(tombstoned_events(&get(router, "/api/activity").await.1), 0);

    // The new revision has no identity applied at it, so the pass still
    // leaves globex out, now as not ready rather than retired.
    let pending = publish(router).await;
    assert_eq!(exclusion(&pending).as_deref(), Some("notReady"), "{pending}");
    assert!(registered(&dir).is_empty());
    assert_eq!(generation(&dir), 2);

    // Once reconciliation applies the reverted revision, the issuer is
    // registered again in a new generation. Nothing in the trail says the
    // tombstone was undone.
    applied(&plane, &reverted);
    let reinstated = publish(router).await;
    assert_eq!(exclusion(&reinstated), None, "{reinstated}");
    assert_eq!(registered(&dir), ["globex"]);
    assert_eq!(generation(&dir), 3);
    assert_eq!(tenants(&dir), ["globex"]);
    assert_eq!(audited(&plane, &globex()).await, audit_before);
}

#[tokio::test]
async fn a_console_tag_from_before_a_client_revert_is_refused_and_writes_nothing() {
    let (_dir, plane, ready) = registered_globex("client-revert-stale-tag").await;
    let router = &plane.router;
    let before_tombstone = plane.repository.get(&globex()).await.unwrap().document;
    let tombstoned = tombstone(router, &ready).await;
    let reverted = commit(&plane, &globex(), &before_tombstone, "tombstone globex").await;

    // An operator whose console still holds the tombstone's tag.
    let response = send(
        router,
        as_operator("PUT", "/api/clients/globex/product")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, format!("\"{}\"", tombstoned.as_str()))
            .body(Body::from(
                json!({"displayName":"Globex","legalName":"Globex Ltd","region":"NZ","timezone":"Pacific/Auckland",
                    "hosts":[],"configuration":{},"applications":[]})
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let stored = plane.repository.get(&globex()).await.unwrap();
    assert_eq!(stored.revision, reverted);
    assert_eq!(stored.document, before_tombstone);
    assert_eq!(
        audited(&plane, &globex()).await,
        ["control_plane.audit.client_tombstoned"]
    );
}

/// Characterizes a removal the console refuses ("Application removal
/// requires deprovisioning"), made by reverting the save that assigned the
/// application.
#[tokio::test]
async fn reverting_an_assignment_removes_what_the_console_refuses_to_remove_and_reaches_no_published_document(
) {
    let (dir, plane, _) = registered_globex("client-revert-assignment").await;
    let router = &plane.router;
    let newco = ClientId::try_new("newco").unwrap();
    let product = |applications: Value| {
        json!({"displayName":"Newco","legalName":"Newco Ltd","region":"NZ","timezone":"Pacific/Auckland",
            "hosts":["newco.example.com"],"configuration":{},"applications":applications})
    };
    let put = |revision: String, body: Value| {
        as_operator("PUT", "/api/clients/newco/product")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .body(Body::from(body.to_string()))
            .unwrap()
    };

    let response = send(
        router,
        as_operator("POST", "/api/clients")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"id":"newco","configuration":product(json!([]))}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let unassigned = plane.repository.get(&newco).await.unwrap();
    let assigned = json!([{"applicationId":"workspec","version":1,"planId":"standard","configuration":{}}]);
    let response = send(
        router,
        put(unassigned.revision.as_str().to_owned(), product(assigned)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let (_, identity) = get(router, "/api/clients/newco/identity").await;
    assert_eq!(identity["clients"][0]["id"], "workspec", "{identity}");

    // The console refuses to take the assignment away.
    let current = plane.repository.get(&newco).await.unwrap();
    let response = send(
        router,
        put(current.revision.as_str().to_owned(), product(json!([]))),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let trail = audited(&plane, &newco).await;

    publish(router).await;
    let before = published(&dir);

    // The revert takes it away: the assignment and the identity client
    // projected from it both go, and nothing is audited.
    commit(&plane, &newco, &unassigned.document, "assign workspec to newco").await;
    let (_, after) = get(router, "/api/clients/newco/product").await;
    assert_eq!(after["product"]["applications"], json!([]), "{after}");
    let (_, identity) = get(router, "/api/clients/newco/identity").await;
    assert_eq!(identity["clients"], json!([]), "{identity}");
    assert_eq!(audited(&plane, &newco).await, trail);

    // newco has no placement, so no published document is touched.
    publish(router).await;
    assert_eq!(published(&dir), before);
}

#[tokio::test]
async fn reverting_a_clients_creation_leaves_its_tenant_published_and_takes_its_issuer_out() {
    let (dir, plane, _) = registered_globex("client-revert-creation").await;
    let router = &plane.router;
    let created = plane.repository.get(&globex()).await.unwrap().document;
    let tenants_before = read(&dir, "tenants.json");

    assert!(plane.repository.remove_by_hand(&globex()));
    let (status, _) = get(router, "/api/clients/globex").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The placement is the platform's record, not the client's, so the
    // tenant binding stays published. Its issuer leaves the registry in a
    // new generation, reported as having no client.
    let removed = publish(router).await;
    assert_eq!(exclusion(&removed).as_deref(), Some("noClient"), "{removed}");
    assert!(registered(&dir).is_empty());
    assert_eq!(generation(&dir), 2);
    assert_eq!(read(&dir, "tenants.json"), tenants_before);
    assert!(audited(&plane, &globex()).await.is_empty());

    // Reverting the revert brings the document back at a new revision.
    // Identity is pending there until reconciliation applies it.
    let restored = plane.repository.insert(&created).unwrap();
    let pending = publish(router).await;
    assert_eq!(exclusion(&pending).as_deref(), Some("notReady"), "{pending}");
    assert_eq!(generation(&dir), 2);
    applied(&plane, &restored);
    publish(router).await;
    assert_eq!(registered(&dir), ["globex"]);
    assert_eq!(generation(&dir), 3);
}
