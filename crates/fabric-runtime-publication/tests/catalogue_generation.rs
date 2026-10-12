//! G8: the catalogue is part of the generation a replica serves. A running
//! replica installs a new catalogue in the same view as the tenants and data
//! sources of its generation, every request reads one pinned view from
//! resource lookup to execution, and a candidate whose catalogue lags its
//! record, does not decode, or is missing holds the whole previous view, with
//! or without a last-known-good copy. Driven through the real
//! `FilesystemRuntimePublication`, the production reader and the real Data
//! API.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines
)]

mod support;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use fabric_connector::{
    CollectionName, CollectionSchema, ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema,
    DataConnector, ExecutionTarget, MutationOutcome, MutationSpec, QueryOutcome, QuerySpec, Row,
};
use fabric_core::{
    BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, OperationKind, TenantId,
};
use fabric_data_api::{
    build_data_api, AuthorizationCheck, CallerCredential, CheckDecision, CheckFailure, CheckQuestion,
    DataApiConfig, DataAuthorization, TenantAuthorization, TenantPolicies, TenantPosture,
};
use fabric_identity::{build_identity, IdentityConfig, TrustedIngressReader};
use fabric_runtime_publication::{
    CatalogDocument, CollectionName as PublishedCollection, ConnectionName, ConnectionSelectorDocument,
    DataSourceDocument, DocumentInput, DocumentRevision, FieldName, IsolationModelDocument,
    ResourceDefinitionDocument, RuntimePublication as _, RuntimeSnapshot, TenantBindingDocument,
    TenantDataBindingDocument, TenantDataBindings, CATALOG_FILE, CATALOG_MANIFEST_FILE, DATA_SOURCES_FILE,
    DATA_SOURCES_MANIFEST_FILE, PUBLICATION_RECORD_FILE, TENANTS_FILE, TENANTS_MANIFEST_FILE,
};
use fabric_tenant_runtime::{
    build_published_runtime, LastKnownGoodState, LastKnownGoodStore, PublishedRuntime, RuntimeConfig,
    RuntimeResolver,
};
use http::StatusCode;
use serde_json::{json, Value};
use tokio::sync::Notify;
use tower::ServiceExt as _;

use support::{body_json, json_request, request, TempDir};

const FIRST_DATA_SOURCE: &str = support::DATA_SOURCE_ID;
const SECOND_DATA_SOURCE: &str = "shared-postgres-02";

fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}

/// One resource on logical `primary`, readable, listable and creatable.
fn resource(collection: &str, fields: &[&str]) -> ResourceDefinitionDocument {
    ResourceDefinitionDocument {
        data_source: LogicalDataSourceName::try_new("primary").unwrap(),
        collection: PublishedCollection::try_new(collection).unwrap(),
        key_field: field("id"),
        operations: vec![OperationKind::Read, OperationKind::List, OperationKind::Create],
        queryable_fields: fields.iter().map(|name| field(name)).collect(),
    }
}

fn catalogue(name: &str, fields: &[&str]) -> CatalogDocument {
    let mut resources = BTreeMap::new();
    resources.insert(
        LogicalResourceName::try_new(name).unwrap(),
        resource(name, fields),
    );
    CatalogDocument::new(resources)
}

fn tenant_on(tenant: &str, discriminator: &str, data_source: &str, revision: u64) -> TenantBindingDocument {
    let mut data = BTreeMap::new();
    data.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new(data_source).unwrap(),
            isolation: IsolationModelDocument::Discriminator {
                column: field(support::DISCRIMINATOR_COLUMN),
                value: discriminator.to_owned(),
            },
        },
    );
    TenantBindingDocument {
        tenant: TenantId::try_new(tenant).unwrap(),
        revision: BindingRevision::new(revision),
        data: TenantDataBindings::try_new(data).unwrap(),
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    }
}

/// The fixture's shared data source, writable, so the write path runs.
fn first_data_source() -> DataSourceDocument {
    let mut source = support::shared_data_source(1);
    source.capabilities.writable = true;
    source
}

fn second_data_source() -> DataSourceDocument {
    let mut source = first_data_source();
    source.id = DataSourceId::try_new(SECOND_DATA_SOURCE).unwrap();
    source.connection = ConnectionSelectorDocument::Named {
        name: ConnectionName::try_new("shared-02").unwrap(),
    };
    source
}

/// Generation 1: `articles` (`id`, `title`) for both tenants, on the first
/// data source.
fn first_generation() -> RuntimeSnapshot {
    RuntimeSnapshot {
        data_sources: DocumentInput::new(DocumentRevision::new(1), vec![first_data_source()]),
        catalog: DocumentInput::new(DocumentRevision::new(1), catalogue("articles", &["id", "title"])),
        ..support::base_snapshot(1)
    }
}

/// Generation 2 moves all three: the catalogue serves `posts` (`id`,
/// `body`) instead of `articles`, and both tenants move to the second data
/// source, behind another connection.
fn second_generation() -> RuntimeSnapshot {
    RuntimeSnapshot {
        tenants: DocumentInput::new(
            DocumentRevision::new(2),
            vec![
                tenant_on("acme", support::ACME_DISCRIMINATOR_VALUE, SECOND_DATA_SOURCE, 2),
                tenant_on(
                    "globex",
                    support::GLOBEX_DISCRIMINATOR_VALUE,
                    SECOND_DATA_SOURCE,
                    2,
                ),
            ],
        ),
        data_sources: DocumentInput::new(
            DocumentRevision::new(2),
            vec![first_data_source(), second_data_source()],
        ),
        catalog: DocumentInput::new(DocumentRevision::new(2), catalogue("posts", &["id", "body"])),
        registry: None,
    }
}

fn claims(scopes: &str) -> Value {
    json!({
        "iss": support::issuer_for("acme"),
        "tenant_id": "acme",
        "scope": scopes,
    })
}

fn articles() -> Value {
    claims("data:articles:read data:articles:write")
}

fn posts() -> Value {
    claims("data:posts:read data:posts:write")
}

/// What one call handed the connector.
#[derive(Debug, Clone)]
struct Reached {
    target: ExecutionTarget,
    collection: String,
}

/// Answers every query with one row carrying every field either catalogue
/// names, and records each query and insert.
struct Recording {
    id: ConnectorId,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
    reached: Mutex<Vec<Reached>>,
}

impl Recording {
    fn new() -> Arc<Self> {
        let columns = || {
            CollectionSchema::new(["id", "title", "body", support::DISCRIMINATOR_COLUMN].map(support::field))
        };
        Arc::new(Self {
            id: ConnectorId::try_new(support::CONNECTOR_ID).unwrap(),
            capabilities: ConnectorCapabilities {
                mutations: true,
                ..ConnectorCapabilities::baseline()
            },
            schema: ConnectorSchema::new([
                (CollectionName::try_new("articles").unwrap(), columns()),
                (CollectionName::try_new("posts").unwrap(), columns()),
            ]),
            reached: Mutex::new(Vec::new()),
        })
    }

    fn last(&self) -> Reached {
        self.reached
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("nothing reached the connector")
    }
}

#[async_trait]
impl DataConnector for Recording {
    fn id(&self) -> &ConnectorId {
        &self.id
    }

    fn capabilities(&self) -> &ConnectorCapabilities {
        &self.capabilities
    }

    fn schema(&self) -> &ConnectorSchema {
        &self.schema
    }

    async fn query(
        &self,
        target: &ExecutionTarget,
        spec: &QuerySpec,
    ) -> Result<QueryOutcome, ConnectorError> {
        self.reached.lock().unwrap().push(Reached {
            target: target.clone(),
            collection: spec.collection.to_string(),
        });
        let row = Row::new()
            .with(support::field("id"), Value::from(1))
            .with(support::field("title"), Value::from("a title"))
            .with(support::field("body"), Value::from("a body"))
            .with(
                support::field(support::DISCRIMINATOR_COLUMN),
                Value::from(support::ACME_DISCRIMINATOR_VALUE),
            );
        Ok(QueryOutcome::from_rows(vec![row]))
    }

    async fn mutate(
        &self,
        target: &ExecutionTarget,
        spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        let MutationSpec::Insert { collection, rows } = spec else {
            panic!("only inserts are driven here, got {spec:?}");
        };
        self.reached.lock().unwrap().push(Reached {
            target: target.clone(),
            collection: collection.to_string(),
        });
        Ok(MutationOutcome::affected(rows.len() as u64))
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        Ok(())
    }
}

/// A relation Check that, once armed, holds the next request inside the
/// authorization await until released: the point where a request has
/// already pinned its view and looked its resource up.
#[derive(Default)]
struct Gate {
    armed: AtomicBool,
    entered: Notify,
    release: Notify,
}

#[async_trait]
impl AuthorizationCheck for Gate {
    async fn check(
        &self,
        _credential: &CallerCredential,
        _question: &CheckQuestion,
    ) -> Result<CheckDecision, CheckFailure> {
        if self.armed.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(CheckDecision::Allowed)
    }
}

/// One replica: the published directory, the production reader over it, and
/// the real Data API in front.
struct Replica {
    dir: TempDir,
    copy: Option<TempDir>,
    runtime: PublishedRuntime,
    loads: Arc<AtomicUsize>,
    app: Router,
    connector: Arc<Recording>,
    gate: Arc<Gate>,
}

impl Replica {
    async fn start(with_copy: bool) -> Self {
        let dir = TempDir::new();
        dir.publisher().publish(&first_generation()).await.unwrap();
        Self::over(dir, with_copy, &support::runtime_config())
            .await
            .unwrap()
    }

    async fn over(dir: TempDir, with_copy: bool, config: &RuntimeConfig) -> Result<Self, String> {
        let copy = with_copy.then(TempDir::new);
        let files = support::published_files(&dir);
        let files = match &copy {
            Some(copy) => files.with_last_known_good(LastKnownGoodStore::new(copy.path().join("copy"))),
            None => files,
        };
        let (source, loads) = support::CountingPublished::over(files);
        let runtime = build_published_runtime(config, Arc::new(source)).await?;

        let connector = Recording::new();
        let gate = Arc::new(Gate::default());
        let acme = TenantAuthorization::new(
            TenantPosture::RelationChecked,
            ["articles", "posts"].map(|name| LogicalResourceName::try_new(name).unwrap()),
        );
        let authorization = DataAuthorization::new(
            TenantPolicies::default().with(TenantId::try_new("acme").unwrap(), acme),
            Arc::clone(&gate) as Arc<dyn AuthorizationCheck>,
        );
        let identity = build_identity(
            IdentityConfig {
                trusted_issuers: support::trusted_issuers(),
                ..IdentityConfig::default()
            },
            Arc::new(TrustedIngressReader::new(Arc::new(support::FixedClock))),
        )
        .unwrap();
        let app = build_data_api(
            &DataApiConfig::default(),
            authorization,
            Arc::clone(&runtime.resolver),
            fabric_connector::ConnectorRegistry::new().with(Arc::clone(&connector) as Arc<dyn DataConnector>),
            identity,
        )
        .unwrap();

        Ok(Self {
            dir,
            copy,
            runtime,
            loads,
            app,
            connector,
            gate,
        })
    }

    fn resolver(&self) -> &RuntimeResolver {
        &self.runtime.resolver
    }

    fn served(&self) -> Option<u64> {
        self.resolver()
            .served_generation()
            .map(|report| report.generation)
    }

    /// Asks for a refresh and waits until a pass has read.
    async fn refresh(&self) {
        let before = self.loads.load(Ordering::SeqCst);
        self.runtime.handles.refresh_now();
        support::poll_for_load_count_above(&self.loads, before, Duration::from_secs(5)).await;
    }

    async fn send(&self, request: http::Request<axum::body::Body>) -> (StatusCode, Value) {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        (status, body_json(response).await)
    }

    fn copy_bytes(&self) -> Option<Vec<u8>> {
        self.copy
            .as_ref()
            .map(|copy| std::fs::read(LastKnownGoodStore::new(copy.path().join("copy")).path()).unwrap())
    }

    async fn shutdown(self) {
        self.runtime.handles.shutdown().await.unwrap();
    }
}

fn data_source_of(reached: &Reached) -> String {
    reached.target.data_source().to_string()
}

/// Asserts the replica serves generation 1 whole: its stamp, its catalogue,
/// its binding and its target, through the Data API.
async fn serves_the_first_generation(replica: &Replica) {
    assert_eq!(replica.served(), Some(1));
    let (status, body) = replica.send(request("GET", "/articles/1", &articles())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["title"], "a title");
    let reached = replica.connector.last();
    assert_eq!(data_source_of(&reached), FIRST_DATA_SOURCE);
    assert_eq!(reached.collection, "articles");
    assert_eq!(reached.target.tenant_revision(), BindingRevision::new(1));
    let (status, _) = replica.send(request("GET", "/posts/1", &posts())).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "generation 2's catalogue is not served"
    );
}

/// Asserts the replica serves generation 2 whole.
async fn serves_the_second_generation(replica: &Replica) {
    assert_eq!(replica.served(), Some(2));
    let (status, body) = replica.send(request("GET", "/posts?select=body", &posts())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"][0]["body"], "a body");
    assert!(
        body["data"][0].get("title").is_none(),
        "generation 2's field rules"
    );
    let reached = replica.connector.last();
    assert_eq!(data_source_of(&reached), SECOND_DATA_SOURCE);
    assert_eq!(reached.collection, "posts");
    assert_eq!(reached.target.tenant_revision(), BindingRevision::new(2));
    let (status, _) = replica.send(request("GET", "/articles/1", &articles())).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "generation 1's catalogue is gone");
}

#[tokio::test]
async fn a_running_replica_installs_a_new_catalogue_with_its_tenants_data_sources_and_stamp() {
    for with_copy in [false, true] {
        let replica = Replica::start(with_copy).await;
        serves_the_first_generation(&replica).await;

        replica
            .dir
            .publisher()
            .publish(&second_generation())
            .await
            .unwrap();
        replica.refresh().await;

        serves_the_second_generation(&replica).await;
        assert_eq!(replica.runtime.status.report().held_passes, 0);
        if with_copy {
            assert_eq!(
                replica.runtime.status.report().last_known_good,
                LastKnownGoodState::Stored { generation: 2 }
            );
        }
        replica.shutdown().await;
    }
}

/// Holds one request in its authorization await, installs generation 2,
/// then lets it go: it must finish on generation 1's resource definition,
/// field rules, binding, DataSource and target, and the next request must
/// read generation 2.
async fn a_request_pinned_on_the_first_generation_reads_it_throughout(
    held: http::Request<axum::body::Body>,
    expect: impl FnOnce(StatusCode, &Value),
) {
    let replica = Arc::new(Replica::start(false).await);
    replica.gate.armed.store(true, Ordering::SeqCst);
    let in_flight = {
        let replica = Arc::clone(&replica);
        tokio::spawn(async move { replica.send(held).await })
    };
    replica.gate.entered.notified().await;

    replica
        .dir
        .publisher()
        .publish(&second_generation())
        .await
        .unwrap();
    replica.refresh().await;
    assert_eq!(
        replica.served(),
        Some(2),
        "generation 2 installed while the request was held"
    );

    replica.gate.release.notify_one();
    let (status, body) = in_flight.await.unwrap();
    expect(status, &body);
    let reached = replica.connector.last();
    assert_eq!(data_source_of(&reached), FIRST_DATA_SOURCE);
    assert_eq!(reached.collection, "articles");
    assert_eq!(reached.target.tenant_revision(), BindingRevision::new(1));
    assert_eq!(reached.target.data_source_revision(), BindingRevision::new(1));

    serves_the_second_generation(&replica).await;
    Arc::into_inner(replica).unwrap().shutdown().await;
}

#[tokio::test]
async fn a_read_pinned_before_a_new_generation_installs_finishes_on_its_own_generation() {
    a_request_pinned_on_the_first_generation_reads_it_throughout(
        request("GET", "/articles?select=title", &articles()),
        |status, body| {
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["data"][0]["title"], "a title", "generation 1's field rules");
            assert!(body["data"][0].get("body").is_none(), "{body}");
        },
    )
    .await;
}

#[tokio::test]
async fn a_write_pinned_before_a_new_generation_installs_finishes_on_its_own_generation() {
    a_request_pinned_on_the_first_generation_reads_it_throughout(
        json_request(
            "POST",
            "/articles",
            &articles(),
            &json!({ "id": 2, "title": "new" }),
        ),
        |status, body| assert!(status.is_success(), "{status}: {body}"),
    )
    .await;
}

/// How a candidate generation 2 can be unusable while its tenants and data
/// sources are fine.
#[derive(Debug, Clone, Copy)]
enum Breakage {
    /// The catalogue volume still holds generation 1's catalogue.
    Lags,
    /// The catalogue matches a record that names it, but the host cannot
    /// decode it.
    Undecodable,
    /// The catalogue file is gone.
    Missing,
}

impl Breakage {
    fn apply(self, dir: &TempDir, first_catalogue: &[u8]) {
        match self {
            Self::Lags => dir.write_raw(CATALOG_FILE, first_catalogue),
            Self::Undecodable => {
                support::edit_json(dir, CATALOG_FILE, |catalog| {
                    catalog["posts"]["page_size"] = json!(50);
                });
                support::rewrite_record(dir, 2, |_| {});
            }
            Self::Missing => dir.remove_raw(CATALOG_FILE),
        }
    }

    const fn named(self) -> &'static str {
        match self {
            Self::Lags => "catalog",
            Self::Undecodable => "does not match the catalogue shape",
            Self::Missing => "could not read",
        }
    }
}

#[tokio::test]
async fn a_candidate_whose_catalogue_lags_does_not_decode_or_is_missing_holds_the_whole_view_until_a_valid_one(
) {
    for breakage in [Breakage::Lags, Breakage::Undecodable, Breakage::Missing] {
        for with_copy in [false, true] {
            let case = format!("{breakage:?}, copy {with_copy}");
            let replica = Replica::start(with_copy).await;
            let first_catalogue = replica.dir.read_raw(CATALOG_FILE);
            let copy_before = replica.copy_bytes();
            replica
                .dir
                .publisher()
                .publish(&second_generation())
                .await
                .unwrap();
            let second = replica.dir.capture(&[CATALOG_FILE, PUBLICATION_RECORD_FILE]);
            breakage.apply(&replica.dir, &first_catalogue);

            replica.refresh().await;

            serves_the_first_generation(&replica).await;
            let report = replica.runtime.status.report();
            assert_eq!(report.held_passes, 1, "{case}");
            let last_hold = report.last_hold.unwrap();
            assert!(last_hold.contains(breakage.named()), "{case}: {last_hold}");
            assert_eq!(replica.copy_bytes(), copy_before, "{case}: the copy is untouched");
            if with_copy {
                assert_eq!(
                    report.last_known_good,
                    LastKnownGoodState::Stored { generation: 1 },
                    "{case}"
                );
            }

            replica.dir.restore(&second);
            replica.refresh().await;
            serves_the_second_generation(&replica).await;
            replica.shutdown().await;
        }
    }
}

#[tokio::test]
async fn a_replica_that_cannot_install_a_first_view_fails_fast_or_serves_nothing_until_one_installs() {
    let without_catalogue = || async {
        let dir = TempDir::new();
        dir.publisher().publish(&first_generation()).await.unwrap();
        let complete = dir.capture(&[CATALOG_FILE]);
        dir.remove_raw(CATALOG_FILE);
        (dir, complete)
    };

    let (dir, _) = without_catalogue().await;
    let refused = Replica::over(dir, false, &support::runtime_config())
        .await
        .err()
        .expect("fail_fast_on_prime refuses a start with no complete view");
    assert!(refused.contains("catalog"), "{refused}");

    let (dir, complete) = without_catalogue().await;

    let unprimed = RuntimeConfig {
        fail_fast_on_prime: false,
        ..support::runtime_config()
    };
    let replica = Replica::over(dir, false, &unprimed).await.unwrap();
    assert_eq!(replica.served(), None, "no generation is reported");
    assert!(
        replica.resolver().pin().catalogue().is_none(),
        "no catalogue of its own"
    );
    let (status, body) = replica.send(request("GET", "/articles/1", &articles())).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let (status, _) = replica.send(request("GET", "/articles/1", &claims(""))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "authorised first, as before");

    replica.dir.restore(&complete);
    replica.refresh().await;
    serves_the_first_generation(&replica).await;
    replica.shutdown().await;
}

#[tokio::test]
async fn with_no_record_a_pass_installs_all_three_together_and_holds_the_whole_view_on_a_bad_catalogue() {
    let dir = TempDir::new();
    dir.publisher().publish(&first_generation()).await.unwrap();
    dir.remove_raw(PUBLICATION_RECORD_FILE);
    let replica = Replica::over(dir, false, &support::runtime_config())
        .await
        .unwrap();
    assert_eq!(
        replica.served(),
        None,
        "documents with no record are no generation"
    );
    let (status, _) = replica.send(request("GET", "/articles/1", &articles())).await;
    assert_eq!(status, StatusCode::OK);

    let staged = TempDir::new();
    staged.publisher().publish(&second_generation()).await.unwrap();
    for file in ["tenants.json", "data-sources.json", CATALOG_FILE] {
        replica.dir.write_raw(file, &staged.read_raw(file));
    }
    replica.refresh().await;
    assert_eq!(replica.served(), None);
    let (status, body) = replica
        .send(request("GET", "/posts/1?select=body", &posts()))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(data_source_of(&replica.connector.last()), SECOND_DATA_SOURCE);

    let back = TempDir::new();
    back.publisher().publish(&first_generation()).await.unwrap();
    replica
        .dir
        .write_raw("tenants.json", &back.read_raw("tenants.json"));
    replica.dir.write_raw(CATALOG_FILE, b"{ torn");
    replica.refresh().await;
    let (status, _) = replica.send(request("GET", "/posts/1", &posts())).await;
    assert_eq!(status, StatusCode::OK, "the whole previous view still serves");
    assert_eq!(data_source_of(&replica.connector.last()), SECOND_DATA_SOURCE);
    assert_eq!(replica.runtime.status.report().held_passes, 1);
    replica.shutdown().await;
}

#[tokio::test]
async fn losing_the_record_after_a_recorded_view_holds_that_view_rather_than_reading_the_documents_alone() {
    for with_copy in [false, true] {
        let replica = Replica::start(with_copy).await;
        let staged = TempDir::new();
        staged.publisher().publish(&second_generation()).await.unwrap();
        for file in ["tenants.json", "data-sources.json", CATALOG_FILE] {
            replica.dir.write_raw(file, &staged.read_raw(file));
        }
        replica.dir.remove_raw(PUBLICATION_RECORD_FILE);

        replica.refresh().await;

        serves_the_first_generation(&replica).await;
        let last_hold = replica.runtime.status.report().last_hold.unwrap();
        assert!(
            last_hold.contains("no publication record"),
            "copy {with_copy}: {last_hold}"
        );
        replica.shutdown().await;
    }
}

/// Every file a publication leaves, to capture a whole published set and put
/// it back.
const PUBLISHED: [&str; 7] = [
    TENANTS_FILE,
    TENANTS_MANIFEST_FILE,
    DATA_SOURCES_FILE,
    DATA_SOURCES_MANIFEST_FILE,
    CATALOG_FILE,
    CATALOG_MANIFEST_FILE,
    PUBLICATION_RECORD_FILE,
];

/// Generation 2 changes only the catalogue: `articles` becomes `posts`,
/// with every tenant and DataSource at the revision generation 1 published,
/// so no resource revision can tell the two apart.
fn catalogue_only() -> RuntimeSnapshot {
    RuntimeSnapshot {
        catalog: DocumentInput::new(DocumentRevision::new(2), catalogue("posts", &["id", "body"])),
        ..first_generation()
    }
}

/// Review of #170: a complete generation below the one installed was taken
/// without a copy, because only a lost record was held against. A
/// catalogue-only rollback is invisible to the resource revision checks, so
/// only the installed generation can refuse it. It is now held, with or
/// without a copy, and a later generation still installs.
#[tokio::test]
async fn a_complete_generation_below_the_one_installed_is_held_with_or_without_a_copy() {
    for with_copy in [false, true] {
        let replica = Replica::start(with_copy).await;
        let first = replica.dir.capture(&PUBLISHED);
        replica.dir.publisher().publish(&catalogue_only()).await.unwrap();
        replica.refresh().await;
        assert_eq!(replica.served(), Some(2), "copy {with_copy}");
        let copy_before = replica.copy_bytes();
        let second = replica.dir.capture(&PUBLISHED);

        replica.dir.restore(&first);
        replica.refresh().await;

        assert_eq!(replica.served(), Some(2), "copy {with_copy}: the stamp stays");
        let (status, _) = replica.send(request("GET", "/posts/1", &posts())).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "copy {with_copy}: generation 2's catalogue serves"
        );
        let (status, _) = replica.send(request("GET", "/articles/1", &articles())).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "copy {with_copy}: generation 1's does not"
        );
        let report = replica.runtime.status.report();
        assert_eq!(report.held_passes, 1, "copy {with_copy}");
        let last_hold = report.last_hold.unwrap();
        assert!(
            last_hold.contains("below generation 2"),
            "copy {with_copy}: {last_hold}"
        );
        assert_eq!(
            replica.copy_bytes(),
            copy_before,
            "copy {with_copy}: the copy is untouched"
        );

        replica.dir.restore(&second);
        replica
            .dir
            .publisher()
            .publish(&second_generation())
            .await
            .unwrap();
        replica.refresh().await;
        assert_eq!(
            replica.served(),
            Some(3),
            "copy {with_copy}: a later generation installs"
        );
        let (status, _) = replica.send(request("GET", "/posts/1", &posts())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(data_source_of(&replica.connector.last()), SECOND_DATA_SOURCE);
        replica.shutdown().await;
    }
}
