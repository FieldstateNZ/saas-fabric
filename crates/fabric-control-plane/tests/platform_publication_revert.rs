//! O5: what a Git revert of the platform's environment files, or a release
//! nobody is placed on yet, does through the real router, the controller's
//! pass and a real filesystem publication target (D01-27: placements roll
//! forward only, referenced data sources change only through
//! revision-bumping writes, no blind revert or removal).
//!
//! A revert is stood in for by seeding `FakeRepository` with an earlier
//! file's content -- what `placements.yaml` or `data-sources.yaml` holds
//! after `git revert`, which reaches the controller as an ordinary read.
//! Every refusal is checked to have written nothing: the documents a
//! reader would load stay byte-for-byte what was last published.

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
use fabric_client_model::catalogue::{
    Application, ApplicationDefinition, ApplicationPlan, ApplicationRelease, ApplicationResource, Catalogue,
};
use fabric_client_model::ClientId;
use fabric_control_plane::{ChangeContext, ClientRepository, PublicationSink};
use fabric_core::{
    BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, OperationKind, TenantId,
};
use fabric_platform_management::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, DataSourceDeclaration, DesiredRevision, IsolationModelDocument,
    PlacementClassDocument, PlacementRecord, PoolSettingsDocument,
};
use fabric_runtime_publication::FilesystemRuntimePublication;
use http::StatusCode;
use serde_json::{json, Value};
use support::platform_fixture::{platform_binding, FakeRepository};
use support::{as_operator, control_plane_with_publication, entity_tag, json as body_of, send, TempDir};

const TENANT: &str = "acme";
const DATA_SOURCE: &str = "shared-postgres-nz-01";
const SPARE: &str = "spare-postgres-nz-02";
const DOCUMENTS: [&str; 4] = [
    "tenants.json",
    "data-sources.json",
    "catalog.json",
    "publication.json",
];

fn sink(dir: &TempDir) -> PublicationSink {
    PublicationSink {
        target: Arc::new(FilesystemRuntimePublication::new(
            dir.path().join("tenants.json"),
            dir.path().join("data-sources.json"),
            dir.path().join("catalog.json"),
        )),
        issuer_origin: None,
    }
}

fn data_source(id: &str, revision: u64, max_connections: u32) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(id).unwrap(),
        revision: BindingRevision::new(revision),
        connector: ConnectorId::try_new("postgres-nz").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").unwrap(),
        },
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: Some("NZ".to_owned()),
        },
        pool: PoolSettingsDocument {
            max_connections,
            ..PoolSettingsDocument::default()
        },
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

fn placement(logical: &str, revision: u64, value: &str) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(TENANT).unwrap(),
        logical: LogicalDataSourceName::try_new(logical).unwrap(),
        revision: BindingRevision::new(revision),
        data_source: DataSourceId::try_new(DATA_SOURCE).unwrap(),
        isolation: IsolationModelDocument::Discriminator {
            column: fabric_platform_management::FieldName::try_new("tenant_key").unwrap(),
            value: value.to_owned(),
        },
        placed_at: "2026-09-18T02:14:00Z".to_owned(),
    }
}

fn application(id: &str, resource: &str, logical: &str) -> Application {
    let definition = ApplicationDefinition {
        name: id.to_owned(),
        plans: vec![ApplicationPlan {
            id: ClientId::try_new("standard").unwrap(),
            name: "Standard".to_owned(),
            description: String::new(),
            features: Vec::new(),
            configuration: BTreeMap::new(),
        }],
        resources: vec![ApplicationResource {
            name: LogicalResourceName::try_new(resource).unwrap(),
            data_source: LogicalDataSourceName::try_new(logical).unwrap(),
            collection: fabric_runtime_publication::CollectionName::try_new(resource).unwrap(),
            key_field: fabric_runtime_publication::FieldName::try_new("id").unwrap(),
            operations: vec![OperationKind::Read],
            queryable_fields: Vec::new(),
        }],
        ..ApplicationDefinition::default()
    };
    Application {
        id: ClientId::try_new(id).unwrap(),
        draft: ApplicationDefinition {
            name: id.to_owned(),
            ..ApplicationDefinition::default()
        },
        releases: vec![ApplicationRelease {
            version: 1,
            note: String::new(),
            published_at: 0,
            definition,
        }],
    }
}

struct Environment {
    dir: TempDir,
    plane: support::TestControlPlane,
    fake: Arc<FakeRepository>,
}

/// A platform whose first publication has landed: `acme` placed on
/// `primary` and `secondary`, the shared data source at revision 3 beside
/// an unplaced spare at revision 2, and one release with a resource on
/// `primary`.
async fn published_environment(label: &str) -> Environment {
    let dir = TempDir::new(label);
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));
    fake.seed(
        DesiredRevision::new("data-sources-1"),
        vec![data_source(DATA_SOURCE, 3, 10), data_source(SPARE, 2, 10)],
    );
    fake.seed_placements(
        DesiredRevision::new("placements-1"),
        vec![
            placement("primary", 1, "acme-primary"),
            placement("secondary", 1, "acme-secondary"),
        ],
    );
    plane
        .repository
        .save_catalogue(
            &Catalogue {
                applications: vec![application("workspec", "customers", "primary")],
                ..Catalogue::default()
            },
            None,
            &change(),
        )
        .await
        .unwrap();
    assert_eq!(pass(&plane.router).await["outcome"], "published");
    Environment { dir, plane, fake }
}

fn change() -> ChangeContext {
    ChangeContext {
        requested_by: "test-fixture".to_owned(),
        summary: "revert".to_owned(),
    }
}

/// Runs one pass through `POST /api/platform/publication` and returns its
/// `lastPass` row.
async fn pass(router: &Router) -> Value {
    let response = send(
        router,
        as_operator("POST", "/api/platform/publication")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    body_of(response).await["lastPass"].clone()
}

fn read(dir: &TempDir, name: &str) -> Vec<u8> {
    std::fs::read(dir.path().join(name)).unwrap()
}

fn published(dir: &TempDir) -> Vec<Vec<u8>> {
    DOCUMENTS.iter().map(|name| read(dir, name)).collect()
}

fn document(dir: &TempDir, name: &str) -> Value {
    serde_json::from_slice(&read(dir, name)).unwrap()
}

fn entry<'a>(list: &'a Value, key: &str, id: &str) -> Option<&'a Value> {
    list.as_array().unwrap().iter().find(|item| item[key] == id)
}

fn assert_refused(row: &Value, words: &str) {
    assert_eq!(row["outcome"], "refused", "{row}");
    let detail = row["detail"].as_str().unwrap();
    assert!(detail.contains(words), "{detail}");
}

#[tokio::test]
async fn reverting_placements_yaml_to_drop_a_published_placement_is_refused_and_writes_nothing() {
    let env = published_environment("revert-removes-placement").await;
    let before = published(&env.dir);

    env.fake.seed_placements(
        DesiredRevision::new("placements-reverted"),
        vec![placement("primary", 1, "acme-primary")],
    );
    assert_refused(&pass(&env.plane.router).await, "no placement records it");
    assert_eq!(published(&env.dir), before);

    env.fake
        .seed_placements(DesiredRevision::new("placements-reverted"), vec![]);
    assert_refused(&pass(&env.plane.router).await, "no placement records it");
    assert_eq!(published(&env.dir), before);
}

#[tokio::test]
async fn reverting_a_placement_record_is_refused_even_when_the_revert_adds_another() {
    let env = published_environment("revert-lowers-placement").await;
    env.fake.seed_placements(
        DesiredRevision::new("placements-2"),
        vec![
            placement("primary", 2, "acme-primary-moved"),
            placement("secondary", 1, "acme-secondary"),
        ],
    );
    assert_eq!(pass(&env.plane.router).await["outcome"], "published");
    let before = published(&env.dir);

    // The revert takes `primary` back to its first record, and the same
    // commit range adds two more: acme's total still rises, 3 -> 4, which
    // the publisher alone would take as an advance.
    env.fake.seed_placements(
        DesiredRevision::new("placements-reverted"),
        vec![
            placement("primary", 1, "acme-primary"),
            placement("secondary", 1, "acme-secondary"),
            placement("audit", 1, "acme-audit"),
            placement("events", 1, "acme-events"),
        ],
    );
    assert_refused(&pass(&env.plane.router).await, "does not advance it");
    assert_eq!(published(&env.dir), before);

    // The explicit write D01-27 asks for: the earlier binding at a revision
    // above the one it replaces.
    env.fake.seed_placements(
        DesiredRevision::new("placements-3"),
        vec![
            placement("primary", 3, "acme-primary"),
            placement("secondary", 1, "acme-secondary"),
            placement("audit", 1, "acme-audit"),
        ],
    );
    assert_eq!(pass(&env.plane.router).await["outcome"], "published");
    let tenants = document(&env.dir, "tenants.json");
    let acme = entry(&tenants, "tenant", TENANT).unwrap();
    assert_eq!(acme["revision"], 5, "{acme}");
    assert_eq!(acme["data"]["primary"]["isolation"]["value"], "acme-primary");
}

#[tokio::test]
async fn reverting_a_data_source_correction_is_refused_until_a_correction_moves_past_it() {
    let env = published_environment("revert-data-source").await;
    env.fake.seed(
        DesiredRevision::new("data-sources-2"),
        vec![data_source(DATA_SOURCE, 4, 20), data_source(SPARE, 2, 10)],
    );
    assert_eq!(pass(&env.plane.router).await["outcome"], "published");
    let before = published(&env.dir);

    env.fake.seed(
        DesiredRevision::new("data-sources-reverted"),
        vec![data_source(DATA_SOURCE, 3, 10), data_source(SPARE, 2, 10)],
    );
    assert_refused(&pass(&env.plane.router).await, "was published at revision 4");
    assert_eq!(published(&env.dir), before);

    // The console's correction of the reverted file is placed above the
    // revision last published, not one above the reverted file's 3.
    let declared = declare(&env.plane.router, DATA_SOURCE, 10).await;
    assert_eq!(
        entry(&declared["dataSources"], "id", DATA_SOURCE).unwrap()["revision"],
        5
    );
    assert_eq!(pass(&env.plane.router).await["outcome"], "published");
    let data_sources = document(&env.dir, "data-sources.json");
    assert_eq!(entry(&data_sources, "id", DATA_SOURCE).unwrap()["revision"], 5);
}

#[tokio::test]
async fn a_data_source_removed_and_declared_again_comes_back_above_its_last_published_revision() {
    let env = published_environment("redeclare-data-source").await;

    let response = send(
        &env.plane.router,
        as_operator("DELETE", &format!("/api/platform/data-sources/{SPARE}"))
            .header(
                "if-match",
                format!("\"{}\"", current_tag(&env.plane.router).await),
            )
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(pass(&env.plane.router).await["outcome"], "published");
    assert!(entry(&document(&env.dir, "data-sources.json"), "id", SPARE).is_none());
    let retired = published(&env.dir);

    // `git revert` of the removal brings the old entry back at revision 2.
    env.fake.seed(
        DesiredRevision::new("data-sources-reverted"),
        vec![data_source(DATA_SOURCE, 3, 10), data_source(SPARE, 2, 10)],
    );
    assert_refused(&pass(&env.plane.router).await, "was published at revision 2");
    assert_eq!(published(&env.dir), retired);

    // Through the console, the same content is written above it.
    let declared = declare(&env.plane.router, SPARE, 10).await;
    assert_eq!(
        entry(&declared["dataSources"], "id", SPARE).unwrap()["revision"],
        3
    );
    assert_eq!(pass(&env.plane.router).await["outcome"], "published");
    let data_sources = document(&env.dir, "data-sources.json");
    assert_eq!(entry(&data_sources, "id", SPARE).unwrap()["revision"], 3);
}

#[tokio::test]
async fn a_release_no_tenant_is_placed_on_waits_while_the_last_catalogue_stays_published() {
    let env = published_environment("unbound-catalogue").await;
    let catalogue_before = read(&env.dir, "catalog.json");
    let stored = env.plane.repository.catalogue().await.unwrap();
    env.plane
        .repository
        .save_catalogue(
            &Catalogue {
                applications: vec![
                    application("workspec", "customers", "primary"),
                    application("audit", "auditEvents", "audit"),
                ],
                ..Catalogue::default()
            },
            stored.revision.as_ref(),
            &change(),
        )
        .await
        .unwrap();

    // Nobody binds `audit`: the catalogue waits; a placement still lands.
    env.fake.seed_placements(
        DesiredRevision::new("placements-2"),
        vec![
            placement("primary", 1, "acme-primary"),
            placement("secondary", 1, "acme-secondary"),
            placement("tertiary", 1, "acme-tertiary"),
        ],
    );
    assert_eq!(pass(&env.plane.router).await["outcome"], "published");
    assert_eq!(read(&env.dir, "catalog.json"), catalogue_before);
    let tenants = document(&env.dir, "tenants.json");
    assert!(entry(&tenants, "tenant", TENANT).unwrap()["data"]["tertiary"].is_object());

    env.fake.seed_placements(
        DesiredRevision::new("placements-3"),
        vec![
            placement("primary", 1, "acme-primary"),
            placement("secondary", 1, "acme-secondary"),
            placement("tertiary", 1, "acme-tertiary"),
            placement("audit", 1, "acme-audit"),
        ],
    );
    assert_eq!(pass(&env.plane.router).await["outcome"], "published");
    assert!(document(&env.dir, "catalog.json").get("auditEvents").is_some());
}

async fn current_tag(router: &Router) -> String {
    let response = send(
        router,
        as_operator("GET", "/api/platform/data-sources")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    entity_tag(&response)
}

/// Declares `id` through `PUT /api/platform/data-sources/{id}`, shaped as
/// [`data_source`], and returns the read that followed.
async fn declare(router: &Router, id: &str, max_connections: u32) -> Value {
    let body = json!({
        "connector": "postgres-nz",
        "connection": {"kind": "named", "name": "shared"},
        "placement": "shared",
        "residency": {"region": "nz", "jurisdiction": "NZ"},
        "pool": {
            "maxConnections": max_connections,
            "idleTimeoutSeconds": PoolSettingsDocument::default().idle_timeout_seconds,
            "acquireTimeoutSeconds": PoolSettingsDocument::default().acquire_timeout_seconds,
        },
        "capabilities": {"writable": true, "acceptsNewTenants": true},
        "discriminator": {"column": "tenant_key"},
        "labels": {},
    });
    let response = send(
        router,
        as_operator("PUT", &format!("/api/platform/data-sources/{id}"))
            .header(http::header::CONTENT_TYPE, "application/json")
            .header("if-match", format!("\"{}\"", current_tag(router).await))
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await;
    let status = response.status();
    let body = body_of(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}
