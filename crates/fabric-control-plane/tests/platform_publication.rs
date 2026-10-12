//! `POST /api/platform/publication` and the `publication` row `GET
//! /api/platform` renders, through the real router (ADR 0023 part 4):
//! composing what the platform has declared, recorded and published into
//! the runtime's three documents, and offering them to a real filesystem
//! target.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    // `allow-indexing-slicing-in-tests` in `clippy.toml` only recognises
    // indexing directly inside a `#[test]`/`#[tokio::test]` function; the
    // JSON navigation here mostly happens one level down, in this file's own
    // async command helpers.
    clippy::indexing_slicing
)]

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::Router;
use fabric_client_model::catalogue::{
    Application, ApplicationDefinition, ApplicationRelease, ApplicationResource, Catalogue,
};
use fabric_control_plane::{ChangeContext, ClientRepository, PublicationSink};
use fabric_core::{
    BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, OperationKind, TenantId,
};
use fabric_platform_management::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, DataSourceDeclaration, DesiredRevision, IsolationModelDocument,
    PlacementClassDocument, PlacementRecord, PoolSettingsDocument,
};
use fabric_runtime_publication::{
    FilesystemRuntimePublication, PublicationError, PublicationReport, PublishedResources,
    PublishedRevisions, RuntimePublication, RuntimeSnapshot,
};
use http::StatusCode;
use serde_json::{json, Value};
use support::platform_fixture::platform_binding;
use support::{
    as_operator, control_plane_with_platform, control_plane_with_publication, json as body_of, send, TempDir,
    FIXED_CLOCK_UNIX_SECONDS,
};
use tokio::sync::Notify;

const TENANT: &str = "acme";
/// Deliberately not equal to [`TENANT`]: `compose` must publish exactly
/// what the record says, never recompute the discriminator value from the
/// tenant id (ADR 0023 part 2's own record-is-the-fact argument would be
/// untested if the seeded value and the tenant id happened to agree).
const DISCRIMINATOR_VALUE: &str = "opaque-7f3";
const DATA_SOURCE: &str = "shared-postgres-nz-01";
const RESOURCE: &str = "customers";

/// The publication sink: a real [`FilesystemRuntimePublication`] over a
/// fresh temp directory, so a test can read back exactly what publishing
/// wrote.
fn sink(dir: &TempDir) -> PublicationSink {
    PublicationSink {
        target: Arc::new(FilesystemRuntimePublication::new(
            dir.path().join("tenants.json"),
            dir.path().join("data-sources.json"),
            dir.path().join("catalog.json"),
        )),
        issuer_origin: None,
        readers: None,
    }
}

/// A shared data source with a discriminator column, declared directly
/// through `FakeRepository::seed` -- the break-glass shape, bypassing
/// `DataSources::declare`'s own validation, exactly as the fixture's own
/// rustdoc describes.
fn declared_data_source() -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(DATA_SOURCE).expect("a valid data source id"),
        revision: BindingRevision::new(3),
        connector: ConnectorId::try_new("postgres-nz").expect("a valid connector id"),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").expect("a valid connection name"),
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
            column: fabric_platform_management::FieldName::try_new("tenant_key").expect("a valid field name"),
        }),
        labels: BTreeMap::new(),
    }
}

/// One of `TENANT`'s placements on [`declared_data_source`], isolated by an
/// opaque discriminator value -- the record is seeded directly (not through
/// `select::pick`, which would allocate the tenant id itself), so a test
/// can control the revision, add a second one, and prove `compose` copies
/// the record's own value verbatim rather than recomputing it.
fn placement(logical: &str, revision: u64) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(TENANT).expect("a valid tenant id"),
        logical: LogicalDataSourceName::try_new(logical).expect("a valid logical name"),
        revision: BindingRevision::new(revision),
        data_source: DataSourceId::try_new(DATA_SOURCE).expect("a valid data source id"),
        isolation: IsolationModelDocument::Discriminator {
            column: fabric_platform_management::FieldName::try_new("tenant_key").expect("a valid field name"),
            value: DISCRIMINATOR_VALUE.to_owned(),
        },
        placed_at: "2026-09-18T02:14:00Z".to_owned(),
    }
}

/// Publishes one release, through the real `/api/catalogue` commands --
/// `docs/delivery.md`'s primary operator workflow -- so the catalogue this
/// pass reads is exactly what an operator would have produced, not a
/// shortcut around the rules that build it.
async fn publish_one_application_with_a_resource(router: &Router) {
    async fn command(router: &Router, revision: &Value, value: &Value) -> Value {
        let builder = as_operator("POST", "/api/catalogue").header("content-type", "application/json");
        let builder = if let Some(revision) = revision.as_str() {
            builder.header("if-match", format!("\"{revision}\""))
        } else {
            builder.header("if-none-match", "*")
        };
        let response = send(router, builder.body(Body::from(value.to_string())).unwrap()).await;
        let status = response.status();
        let body = body_of(response).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }

    let created = command(
        router,
        &Value::Null,
        &json!({"action":"createApplication","id":"workspec","name":"Workspec"}),
    )
    .await;
    let definition = json!({
        "name": "Workspec", "description": "", "domain": "{client}.example.com",
        "components": [], "features": [],
        "plans": [{"id":"standard","name":"Standard","description":"","features":[],"configuration":{}}],
        "fields": [], "navigation": [],
        "resources": [{
            "name": RESOURCE, "dataSource": "primary", "collection": RESOURCE, "keyField": "id",
            "operations": ["read", "list"], "queryableFields": [],
        }],
    });
    let saved = command(
        router,
        &created["revision"],
        &json!({"action":"saveApplication","id":"workspec","definition":definition}),
    )
    .await;
    command(
        router,
        &saved["revision"],
        &json!({"action":"publishApplication","id":"workspec","note":"v1"}),
    )
    .await;
}

async fn post_publication(router: &Router) -> (StatusCode, Value) {
    let response = send(
        router,
        as_operator("POST", "/api/platform/publication")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let status = response.status();
    (status, body_of(response).await)
}

async fn get_platform(router: &Router) -> Value {
    let response = send(
        router,
        as_operator("GET", "/api/platform").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    body_of(response).await
}

fn modified(path: &std::path::Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .unwrap_or_else(|error| panic!("{} must exist: {error}", path.display()))
        .modified()
        .expect("mtime must be readable on this platform")
}

#[tokio::test]
async fn publishing_composes_declared_state_into_the_runtimes_three_documents_and_settles_unchanged() {
    let dir = TempDir::new("publication-happy-path");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));

    fake.seed(
        DesiredRevision::new("data-sources-1"),
        vec![declared_data_source()],
    );
    fake.seed_placements(
        DesiredRevision::new("placements-1"),
        vec![placement("primary", 1)],
    );
    publish_one_application_with_a_resource(&plane.router).await;

    // 1. First pass: everything is composed and offered for the first time.
    let (status, body) = post_publication(&plane.router).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["target"].as_str().unwrap().contains("tenants"), "{body}");
    assert_eq!(body["lastPass"]["outcome"], "published", "{body}");
    assert_eq!(body["documents"]["tenants"], 1);
    assert_eq!(body["documents"]["dataSources"], 1);
    assert_eq!(body["documents"]["catalog"], 1);
    // One clock: the trigger renders its response from the moment
    // `publish_once` itself recorded the pass at, never a fresh
    // `SystemClock` read of its own -- so this must be the fixture clock's
    // value, not merely *some* timestamp.
    assert_eq!(
        body["lastPass"]["atUnixSeconds"], FIXED_CLOCK_UNIX_SECONDS,
        "{body}"
    );

    let tenants_path = dir.path().join("tenants.json");
    let data_sources_path = dir.path().join("data-sources.json");
    let catalog_path = dir.path().join("catalog.json");
    for path in [
        &tenants_path,
        &data_sources_path,
        &catalog_path,
        &dir.path().join("tenants.manifest.json"),
        &dir.path().join("data-sources.manifest.json"),
        &dir.path().join("catalog.manifest.json"),
    ] {
        assert!(path.exists(), "{} must exist", path.display());
    }

    let tenants: Value =
        serde_json::from_str(&std::fs::read_to_string(&tenants_path).unwrap()).expect("valid JSON");
    let acme = tenants
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["tenant"] == TENANT)
        .unwrap_or_else(|| panic!("{TENANT} must be published: {tenants}"));
    assert_eq!(acme["revision"], 1, "{acme}");
    assert_eq!(acme["data"]["primary"]["data_source"], DATA_SOURCE, "{acme}");
    assert_eq!(
        acme["data"]["primary"]["isolation"]["kind"], "discriminator",
        "{acme}"
    );
    assert_eq!(
        acme["data"]["primary"]["isolation"]["value"], DISCRIMINATOR_VALUE,
        "{acme}"
    );

    let data_sources: Value =
        serde_json::from_str(&std::fs::read_to_string(&data_sources_path).unwrap()).expect("valid JSON");
    let declared = data_sources
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == DATA_SOURCE)
        .unwrap_or_else(|| panic!("{DATA_SOURCE} must be published: {data_sources}"));
    assert!(
        declared.get("discriminator").is_none(),
        "the discriminator column is not on the wire's DataSourceDocument: {declared}"
    );

    let catalog: Value =
        serde_json::from_str(&std::fs::read_to_string(&catalog_path).unwrap()).expect("valid JSON");
    assert!(catalog.get(RESOURCE).is_some(), "{catalog}");

    let tenants_mtime = modified(&tenants_path);
    let data_sources_mtime = modified(&data_sources_path);
    let catalog_mtime = modified(&catalog_path);

    // 2. Second pass: everything already matches what is held, so nothing
    // moves -- not even a manifest. The no-op property does not rest on
    // mtimes alone: the reported revision must also still read 1, not just
    // "unchanged" in name.
    let (status, body) = post_publication(&plane.router).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["lastPass"]["outcome"], "unchanged", "{body}");
    assert_eq!(body["documents"]["tenants"], 1, "{body}");
    assert_eq!(
        modified(&tenants_path),
        tenants_mtime,
        "tenants.json must not move"
    );
    assert_eq!(
        modified(&data_sources_path),
        data_sources_mtime,
        "data-sources.json must not move"
    );
    assert_eq!(
        modified(&catalog_path),
        catalog_mtime,
        "catalog.json must not move"
    );

    // 3. A second placement for the same tenant, on a second logical data
    // source -- the tenant's published revision is the sum of its records'
    // (ADR 0023 part 4, D1): 1 + 1 = 2.
    fake.seed_placements(
        DesiredRevision::new("placements-2"),
        vec![placement("primary", 1), placement("secondary", 1)],
    );

    let (status, body) = post_publication(&plane.router).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["lastPass"]["outcome"], "published", "{body}");
    assert_eq!(body["documents"]["tenants"], 2);

    let tenants: Value =
        serde_json::from_str(&std::fs::read_to_string(&tenants_path).unwrap()).expect("valid JSON");
    let acme = tenants
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["tenant"] == TENANT)
        .unwrap();
    assert_eq!(acme["revision"], 2, "{acme}");
    assert!(acme["data"]["secondary"].is_object(), "{acme}");

    // 4. `GET /api/platform` shows the same state the trigger itself just
    // reported -- including the moment it happened at: the same fixture
    // clock value every earlier trigger's response also carried, since
    // `GET` reads the very `PublicationState` `publish_once` itself wrote
    // it into rather than a fresh clock read of its own.
    let body = get_platform(&plane.router).await;
    assert_eq!(body["publication"]["lastPass"]["outcome"], "published", "{body}");
    assert_eq!(body["publication"]["documents"]["tenants"], 2, "{body}");
    assert_eq!(
        body["publication"]["lastPass"]["atUnixSeconds"], FIXED_CLOCK_UNIX_SECONDS,
        "{body}"
    );
    assert!(body["publication"]["target"]
        .as_str()
        .unwrap()
        .contains("tenants"));
}

#[tokio::test]
async fn a_catalogue_with_no_published_resources_is_waiting_and_writes_nothing() {
    let dir = TempDir::new("publication-waiting");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));

    fake.seed(
        DesiredRevision::new("data-sources-1"),
        vec![declared_data_source()],
    );
    fake.seed_placements(
        DesiredRevision::new("placements-1"),
        vec![placement("primary", 1)],
    );
    // Deliberately no catalogue release published.

    let (status, body) = post_publication(&plane.router).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["lastPass"]["outcome"], "waiting", "{body}");
    assert!(!dir.path().join("tenants.json").exists());
    assert!(!dir.path().join("data-sources.json").exists());
    assert!(!dir.path().join("catalog.json").exists());
}

#[tokio::test]
async fn a_hand_edited_catalogue_conflict_is_refused_and_writes_nothing() {
    let dir = TempDir::new("publication-conflict");
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_publication(platform, sink(&dir));

    fake.seed(
        DesiredRevision::new("data-sources-1"),
        vec![declared_data_source()],
    );
    fake.seed_placements(
        DesiredRevision::new("placements-1"),
        vec![placement("primary", 1)],
    );

    // A shape release publication itself already refuses -- reachable only
    // by a hand edit outside the console (ADR 0023 part 3). Written
    // directly through the repository, the same way `FakeRepository::seed`
    // bypasses `DataSources::declare`'s own validation for the platform
    // half.
    let resource = || ApplicationResource {
        name: LogicalResourceName::try_new(RESOURCE).unwrap(),
        data_source: LogicalDataSourceName::try_new("primary").unwrap(),
        collection: fabric_runtime_publication::CollectionName::try_new(RESOURCE).unwrap(),
        key_field: fabric_runtime_publication::FieldName::try_new("id").unwrap(),
        operations: vec![OperationKind::Read],
        queryable_fields: Vec::new(),
    };
    // A release publication would refuse this pair outright -- `Catalogue::validate`
    // (called by `render`, which `save_catalogue` runs before storing) does
    // not check cross-application conflicts, only that each application is
    // *individually* publishable, so every other field here has to be a
    // genuinely valid published release for `render` to accept it at all.
    let application = |id: &str| Application {
        id: fabric_client_model::ClientId::try_new(id).unwrap(),
        draft: ApplicationDefinition {
            name: id.to_owned(),
            ..ApplicationDefinition::default()
        },
        releases: vec![ApplicationRelease {
            version: 1,
            note: String::new(),
            published_at: 0,
            definition: ApplicationDefinition {
                name: id.to_owned(),
                plans: vec![fabric_client_model::catalogue::ApplicationPlan {
                    id: fabric_client_model::ClientId::try_new("standard").unwrap(),
                    name: "Standard".to_owned(),
                    description: String::new(),
                    features: Vec::new(),
                    configuration: BTreeMap::new(),
                }],
                resources: vec![resource()],
                ..ApplicationDefinition::default()
            },
        }],
    };
    let catalogue = Catalogue {
        applications: vec![application("workspec"), application("other")],
        ..Catalogue::default()
    };
    plane
        .repository
        .save_catalogue(
            &catalogue,
            None,
            &ChangeContext {
                requested_by: "test-fixture".to_owned(),
                summary: "hand-edited conflict".to_owned(),
            },
        )
        .await
        .expect("the repository accepts whatever it is handed directly");

    let (status, body) = post_publication(&plane.router).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["lastPass"]["outcome"], "refused", "{body}");
    let detail = body["lastPass"]["detail"].as_str().expect("a refusal names why");
    assert!(detail.contains(RESOURCE), "{detail}");
    assert!(!dir.path().join("tenants.json").exists());
    assert!(!dir.path().join("data-sources.json").exists());
    assert!(!dir.path().join("catalog.json").exists());
}

#[tokio::test]
async fn no_publication_target_configured_answers_not_found_and_is_not_retryable() {
    let (platform, _fake) = platform_binding().await;
    let plane = control_plane_with_platform(platform);

    let response = send(
        &plane.router,
        as_operator("POST", "/api/platform/publication")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(
        response.headers().get(http::header::RETRY_AFTER).is_none(),
        "only a config edit and a restart change this; retrying now must not look useful"
    );
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "publication_not_configured");
}

/// Wraps a real [`FilesystemRuntimePublication`], but `current` notifies
/// `entered` and waits on `release` first -- long enough for a concurrent
/// second trigger to observe the re-entry guard held, the same technique
/// `fabric-platform-management`'s own `publisher_tests.rs` uses at the unit
/// level.
struct GatedTarget {
    inner: FilesystemRuntimePublication,
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait::async_trait]
impl RuntimePublication for GatedTarget {
    async fn current(&self) -> Result<PublishedRevisions, PublicationError> {
        self.entered.notify_one();
        self.release.notified().await;
        self.inner.current().await
    }

    async fn published_resources(&self) -> Result<PublishedResources, PublicationError> {
        self.inner.published_resources().await
    }

    async fn publish(&self, snapshot: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
        self.inner.publish(snapshot).await
    }

    async fn inspect(&self) -> fabric_runtime_publication::PublicationCompleteness {
        self.inner.inspect().await
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

#[tokio::test]
async fn a_pass_already_running_answers_409_publication_running() {
    let dir = TempDir::new("publication-running");
    let (platform, fake) = platform_binding().await;

    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let gated = GatedTarget {
        inner: FilesystemRuntimePublication::new(
            dir.path().join("tenants.json"),
            dir.path().join("data-sources.json"),
            dir.path().join("catalog.json"),
        ),
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
    };
    let plane = control_plane_with_publication(
        platform,
        PublicationSink {
            target: Arc::new(gated),
            issuer_origin: None,
            readers: None,
        },
    );

    fake.seed(
        DesiredRevision::new("data-sources-1"),
        vec![declared_data_source()],
    );
    fake.seed_placements(
        DesiredRevision::new("placements-1"),
        vec![placement("primary", 1)],
    );
    publish_one_application_with_a_resource(&plane.router).await;

    let router = plane.router.clone();
    let holder = tokio::spawn(async move { post_publication(&router).await });

    entered.notified().await;

    // A second trigger, while the first is gated mid-pass, must be refused
    // -- not queued behind it.
    let (status, body) = post_publication(&plane.router).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "publication_running");

    release.notify_one();
    let (first_status, first_body) = holder.await.expect("the gated request completes");
    assert_eq!(first_status, StatusCode::OK, "{first_body}");
}

#[tokio::test]
async fn get_platform_reports_publication_null_when_no_sink_is_configured() {
    let (platform, _fake) = platform_binding().await;
    let plane = control_plane_with_platform(platform);

    let body = get_platform(&plane.router).await;
    assert!(body["publication"].is_null(), "{body}");
}

#[tokio::test]
async fn the_trigger_needs_an_operator() {
    let (platform, _fake) = platform_binding().await;
    let dir = TempDir::new("publication-unauthenticated");
    let plane = control_plane_with_publication(platform, sink(&dir));

    let response = send(
        &plane.router,
        http::Request::builder()
            .method("POST")
            .uri("/api/platform/publication")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// A reader observer that answers with whatever the test last told it: every
/// replica reports `report`, at `desired` replicas.
#[derive(Default)]
struct ToldReaders {
    told: std::sync::Mutex<Option<(u32, Option<fabric_core::ReaderGenerationReport>)>>,
}

#[async_trait::async_trait]
impl fabric_platform_management::ReaderObserver for ToldReaders {
    async fn sample(&self) -> fabric_platform_management::ReaderSample {
        use fabric_platform_management::{MemberState, ProbeOutcome, ReaderMember, SampleOutcome};
        let told = *self.told.lock().unwrap();
        let (desired, report) = told.unwrap_or((0, None));
        let members = report
            .map(|report| {
                (0..desired)
                    .map(|index| ReaderMember {
                        pod: format!("fabric-api-{index}"),
                        replica_set: "fabric-api-7d9".to_owned(),
                        state: MemberState::Probed(ProbeOutcome::Reported(report)),
                    })
                    .collect()
            })
            .unwrap_or_default();
        fabric_platform_management::ReaderSample {
            observed_at_unix_seconds: FIXED_CLOCK_UNIX_SECONDS,
            outcome: SampleOutcome::Observed {
                desired_replicas: desired,
                rollout_observed: true,
                members,
            },
        }
    }
}

async fn published_with(readers: Option<Arc<ToldReaders>>) -> (Router, Value, TempDir) {
    let dir = TempDir::new("publication-readers");
    let (platform, fake) = platform_binding().await;
    let sink = PublicationSink {
        readers: readers.map(|readers| readers as Arc<dyn fabric_platform_management::ReaderObserver>),
        ..sink(&dir)
    };
    let plane = control_plane_with_publication(platform, sink);
    fake.seed(
        DesiredRevision::new("data-sources-1"),
        vec![declared_data_source()],
    );
    fake.seed_placements(
        DesiredRevision::new("placements-1"),
        vec![placement("primary", 1)],
    );
    publish_one_application_with_a_resource(&plane.router).await;
    let (status, body) = post_publication(&plane.router).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (plane.router, body, dir)
}

#[tokio::test]
async fn with_no_reader_observer_the_readers_are_unobserved_and_no_update_is_complete() {
    let (router, posted, _dir) = published_with(None).await;
    for body in [posted, get_platform(&router).await["publication"].clone()] {
        let readiness = &body["readiness"];
        assert_eq!(readiness["documents"]["met"], true, "{body}");
        assert_eq!(readiness["readers"]["state"], "unobserved", "{body}");
        assert!(readiness["readers"]["reasons"][0]
            .as_str()
            .unwrap()
            .contains("no reader observation is configured"));
        assert_eq!(readiness["readers"]["expected"]["generation"], 1, "{body}");
        assert_eq!(readiness["readers"]["observedAtUnixSeconds"], Value::Null);
        assert_eq!(readiness["updateComplete"], false, "{body}");
    }
}

#[tokio::test]
async fn the_stamped_record_is_the_one_on_the_target_and_matching_readers_complete_the_update() {
    let readers = Arc::new(ToldReaders::default());
    let (router, posted, dir) = published_with(Some(Arc::clone(&readers))).await;
    let expected = posted["readiness"]["readers"]["expected"].clone();
    assert_eq!(expected["generation"], 1);
    let digest = fabric_core::RecordDigest::parse(expected["recordSha256"].as_str().unwrap()).unwrap();
    assert!(dir.path().join("publication.json").exists());

    let report = |generation: u64, digest, origin| {
        fabric_core::ReaderGenerationReport::new(generation, digest, origin)
    };
    *readers.told.lock().unwrap() = Some((2, Some(report(1, digest, fabric_core::ServedOrigin::Published))));
    let body = get_platform(&router).await;
    let readiness = &body["publication"]["readiness"];
    assert_eq!(readiness["readers"]["state"], "applied", "{body}");
    assert_eq!(readiness["readers"]["members"][0]["finding"], "matches", "{body}");
    assert_eq!(readiness["readers"]["members"][0]["replicaSet"], "fabric-api-7d9");
    assert_eq!(
        readiness["readers"]["observedAtUnixSeconds"],
        FIXED_CLOCK_UNIX_SECONDS
    );
    assert_eq!(readiness["updateComplete"], true, "{body}");

    *readers.told.lock().unwrap() = Some((
        2,
        Some(report(1, digest, fabric_core::ServedOrigin::LastKnownGood)),
    ));
    let held = get_platform(&router).await;
    assert_eq!(
        held["publication"]["readiness"]["readers"]["state"], "pending",
        "{held}"
    );
    assert_eq!(held["publication"]["readiness"]["updateComplete"], false);

    let other = fabric_core::RecordDigest::from_bytes([0; 32]);
    *readers.told.lock().unwrap() = Some((2, Some(report(1, other, fabric_core::ServedOrigin::Published))));
    let diverged = get_platform(&router).await;
    assert_eq!(
        diverged["publication"]["readiness"]["readers"]["state"], "unobserved",
        "{diverged}"
    );
    assert_eq!(
        diverged["publication"]["readiness"]["readers"]["members"][0]["finding"],
        "wrongDigest"
    );
}

#[tokio::test]
async fn at_zero_replicas_the_documents_gate_still_stands_and_there_are_no_readers() {
    let readers = Arc::new(ToldReaders::default());
    let (router, _posted, _dir) = published_with(Some(readers)).await;
    let body = get_platform(&router).await;
    let readiness = &body["publication"]["readiness"];
    assert_eq!(readiness["documents"]["met"], true, "{body}");
    assert_eq!(readiness["readers"]["state"], "noReaders", "{body}");
    assert_eq!(readiness["readers"]["desiredReplicas"], 0);
    assert_eq!(readiness["updateComplete"], false, "{body}");
}

/// A real [`FilesystemRuntimePublication`] that counts every call through
/// the port, so a test can show `GET /api/platform` reads nothing from the
/// target.
struct CountingTarget {
    inner: FilesystemRuntimePublication,
    calls: Arc<std::sync::atomic::AtomicUsize>,
}

impl CountingTarget {
    fn count(&self) {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl RuntimePublication for CountingTarget {
    async fn current(&self) -> Result<PublishedRevisions, PublicationError> {
        self.count();
        self.inner.current().await
    }

    async fn published_resources(&self) -> Result<PublishedResources, PublicationError> {
        self.count();
        self.inner.published_resources().await
    }

    async fn publish(&self, snapshot: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
        self.count();
        self.inner.publish(snapshot).await
    }

    async fn inspect(&self) -> fabric_runtime_publication::PublicationCompleteness {
        self.count();
        self.inner.inspect().await
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

/// A plane publishing to `dir` through a [`CountingTarget`], with the
/// fixture's data source, placement and release, and the target's call
/// count.
async fn counted_plane(
    dir: &TempDir,
) -> (
    support::TestControlPlane,
    Arc<support::platform_fixture::FakeRepository>,
    Arc<std::sync::atomic::AtomicUsize>,
) {
    let (platform, fake) = platform_binding().await;
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let target = CountingTarget {
        inner: FilesystemRuntimePublication::new(
            dir.path().join("tenants.json"),
            dir.path().join("data-sources.json"),
            dir.path().join("catalog.json"),
        ),
        calls: Arc::clone(&calls),
    };
    let plane = control_plane_with_publication(
        platform,
        PublicationSink {
            target: Arc::new(target),
            issuer_origin: None,
            readers: None,
        },
    );
    fake.seed(
        DesiredRevision::new("data-sources-1"),
        vec![declared_data_source()],
    );
    fake.seed_placements(
        DesiredRevision::new("placements-1"),
        vec![placement("primary", 1)],
    );
    publish_one_application_with_a_resource(&plane.router).await;
    (plane, fake, calls)
}

/// The fixture's data source, changed and advanced, so a pass writes the
/// data-sources document before anything else.
fn relabelled_data_source() -> DataSourceDeclaration {
    DataSourceDeclaration {
        revision: BindingRevision::new(4),
        labels: BTreeMap::from([("tier".to_owned(), "gold".to_owned())]),
        ..declared_data_source()
    }
}

/// Blocks one staging path with a directory, so every write through it
/// fails `Unwritable`, on every immediate re-offer too.
fn block(dir: &TempDir, staging: &str) {
    std::fs::create_dir(dir.path().join(staging)).unwrap();
}

fn unblock(dir: &TempDir, staging: &str) {
    std::fs::remove_dir(dir.path().join(staging)).unwrap();
}

#[tokio::test]
async fn a_pass_that_leaves_the_target_part_written_shows_it_incomplete_and_holds_the_documents_gate() {
    let dir = TempDir::new("publication-incomplete");
    let (plane, fake, calls) = counted_plane(&dir).await;

    let (_, first) = post_publication(&plane.router).await;
    assert_eq!(first["completeness"]["state"], "complete", "{first}");
    assert_eq!(first["completeness"]["record"]["generation"], 1, "{first}");
    assert_eq!(first["completeness"]["reason"], Value::Null, "{first}");
    assert_eq!(first["readiness"]["documents"]["met"], true, "{first}");

    fake.seed(
        DesiredRevision::new("data-sources-2"),
        vec![relabelled_data_source()],
    );
    block(&dir, ".publication.json.tmp");
    let (status, posted) = post_publication(&plane.router).await;
    assert_eq!(status, StatusCode::OK, "{posted}");

    let before_get = calls.load(std::sync::atomic::Ordering::SeqCst);
    let got = get_platform(&plane.router).await["publication"].clone();
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        before_get,
        "GET renders the last inspection and reads nothing from the target"
    );
    for body in [&posted, &got] {
        assert_eq!(body["lastPass"]["outcome"], "failed", "{body}");
        let completeness = &body["completeness"];
        assert_eq!(completeness["state"], "incomplete", "{body}");
        assert_eq!(completeness["record"]["generation"], 1, "{body}");
        assert_eq!(completeness["observedAtUnixSeconds"], FIXED_CLOCK_UNIX_SECONDS);
        assert_eq!(
            completeness["reason"],
            "the target does not hold generation 1 whole: data-sources manifest is at another \
             revision than the record names; data-sources payload does not match the record's digest",
            "{body}"
        );
        assert_eq!(body["readiness"]["documents"]["met"], false, "{body}");
        assert_eq!(
            body["readiness"]["documents"]["reasons"][0], completeness["reason"],
            "{body}"
        );
        assert_eq!(body["readiness"]["updateComplete"], false, "{body}");
        let text = body.to_string();
        assert!(
            !text.contains("gold") && !text.contains(".publication.json.tmp"),
            "{body}"
        );
    }

    unblock(&dir, ".publication.json.tmp");
    let (_, completed) = post_publication(&plane.router).await;
    // The data sources were already written by the interrupted pass, and the
    // pass outcome counts documents, not the record, so this pass reads
    // `unchanged`: only the completeness row shows generation 2 completing.
    assert_eq!(completed["lastPass"]["outcome"], "unchanged", "{completed}");
    assert_eq!(completed["completeness"]["state"], "complete", "{completed}");
    assert_eq!(
        completed["completeness"]["record"]["generation"], 2,
        "{completed}"
    );
    assert_eq!(completed["readiness"]["documents"]["met"], true, "{completed}");
}

#[tokio::test]
async fn a_pass_refused_before_it_wrote_anything_leaves_a_complete_target_complete() {
    let dir = TempDir::new("publication-zero-write");
    let (plane, fake, _calls) = counted_plane(&dir).await;
    post_publication(&plane.router).await;

    fake.seed(
        DesiredRevision::new("data-sources-2"),
        vec![relabelled_data_source()],
    );
    block(&dir, ".data-sources.json.tmp");
    let (_, body) = post_publication(&plane.router).await;

    assert_eq!(body["lastPass"]["outcome"], "failed", "{body}");
    assert_eq!(body["completeness"]["state"], "complete", "{body}");
    assert_eq!(body["completeness"]["record"]["generation"], 1, "{body}");
    assert_eq!(body["readiness"]["documents"]["met"], true, "{body}");
}

#[tokio::test]
async fn a_restarted_control_plane_shows_completeness_unobserved_until_a_pass_inspects_the_target() {
    let dir = TempDir::new("publication-restart");
    {
        let (plane, fake, _calls) = counted_plane(&dir).await;
        post_publication(&plane.router).await;
        fake.seed(
            DesiredRevision::new("data-sources-2"),
            vec![relabelled_data_source()],
        );
        block(&dir, ".publication.json.tmp");
        let (_, body) = post_publication(&plane.router).await;
        assert_eq!(body["completeness"]["state"], "incomplete", "{body}");
    }

    let (plane, fake, calls) = counted_plane(&dir).await;
    let body = get_platform(&plane.router).await["publication"].clone();
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(body["completeness"]["state"], "unobserved", "{body}");
    assert_eq!(body["completeness"]["observedAtUnixSeconds"], Value::Null);
    assert_eq!(
        body["completeness"]["reason"],
        "no publication pass has inspected the target since the control plane started"
    );
    assert_eq!(body["readiness"]["documents"]["met"], false, "{body}");

    fake.seed(
        DesiredRevision::new("data-sources-2"),
        vec![relabelled_data_source()],
    );
    let (_, body) = post_publication(&plane.router).await;
    assert_eq!(body["completeness"]["state"], "incomplete", "{body}");
    assert_eq!(body["readiness"]["documents"]["met"], false, "{body}");
}
