//! Pins what publication guarantees about atomicity, and what it does not,
//! for issue #112 and `docs/roadmap/m2-publication-gap-report.md`.
//!
//! Every test here drives the real `FilesystemRuntimePublication` and the
//! production reader, `build_published_runtime` over `PublishedFiles`, which
//! applies a complete generation -- the documents the publication record
//! names -- or nothing. A Kubernetes volume the kubelet has not yet
//! projected is stood in for by putting a file's previous bytes back; the
//! record travels in the tenants volume, so it is restored with
//! `tenants.json`. Tests whose name
//! starts with `current_behaviour_` assert a behaviour the gap report names
//! as a gap: they pass because the gap exists, and the fix for that gap is
//! expected to change them. They are not ignored, so a change that closes
//! (or widens) a gap is seen in review rather than discovered later. A gap
//! whose engineering part has been fixed keeps a `current_behaviour_` test
//! for what remains, beside a test of the fixed behaviour.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, DataSourceDocument, DocumentInput, DocumentKind,
    DocumentOutcome, DocumentRevision, FieldName, IsolationModelDocument, PublicationError,
    PublishedRevisions, RuntimePublication as _, RuntimeSnapshot, TenantBindingDocument,
    TenantDataBindingDocument, TenantDataBindings, PUBLICATION_RECORD_FILE,
};
use fabric_tenant_runtime::{ResolveError, RuntimeResolver};
use http::StatusCode;
use tower::ServiceExt as _;

const SECOND_DATA_SOURCE: &str = "shared-postgres-02";

fn logical_primary() -> LogicalDataSourceName {
    LogicalDataSourceName::try_new("primary").unwrap()
}

fn tenant_id(name: &str) -> TenantId {
    TenantId::try_new(name).unwrap()
}

/// A second shared DataSource on its own named connection, so it is a
/// distinct destination from the fixture's `shared-postgres-01`.
fn second_data_source(revision: u64) -> DataSourceDocument {
    let mut source = support::shared_data_source(revision);
    source.id = DataSourceId::try_new(SECOND_DATA_SOURCE).unwrap();
    source.connection = ConnectionSelectorDocument::Named {
        name: ConnectionName::try_new("shared-02").unwrap(),
    };
    source
}

fn tenant_on(
    tenant: &str,
    discriminator_value: &str,
    data_source: &str,
    revision: u64,
) -> TenantBindingDocument {
    let mut data = BTreeMap::new();
    data.insert(
        logical_primary(),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new(data_source).unwrap(),
            isolation: IsolationModelDocument::Discriminator {
                column: FieldName::try_new(support::DISCRIMINATOR_COLUMN).unwrap(),
                value: discriminator_value.to_owned(),
            },
        },
    );
    TenantBindingDocument {
        tenant: tenant_id(tenant),
        revision: BindingRevision::new(revision),
        data: TenantDataBindings::try_new(data).unwrap(),
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    }
}

/// Revision 2 of the fixture: `shared-postgres-02` is added, and acme is
/// moved onto it. Correct by every publisher rule: the data source is in the
/// same snapshot as the binding that names it.
fn acme_moves_to_second_data_source() -> RuntimeSnapshot {
    RuntimeSnapshot {
        tenants: DocumentInput::new(
            DocumentRevision::new(2),
            vec![
                tenant_on("acme", support::ACME_DISCRIMINATOR_VALUE, SECOND_DATA_SOURCE, 2),
                tenant_on(
                    "globex",
                    support::GLOBEX_DISCRIMINATOR_VALUE,
                    support::DATA_SOURCE_ID,
                    1,
                ),
            ],
        ),
        data_sources: DocumentInput::new(
            DocumentRevision::new(2),
            vec![support::shared_data_source(1), second_data_source(1)],
        ),
        catalog: DocumentInput::new(DocumentRevision::new(1), support::articles_catalog()),
        registry: None,
    }
}

async fn poll_until(what: &str, deadline: Duration, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(
            start.elapsed() < deadline,
            "{what} did not happen within {deadline:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn resolves_to(resolver: &RuntimeResolver, tenant: &str) -> Result<String, ResolveError> {
    resolver
        .resolve_data_source(&tenant_id(tenant), &logical_primary())
        .map(|resolved| resolved.data_source.id.to_string())
}

/// Every observation of acme, sampled until `until` holds, must be one of
/// `allowed`: proves no sample in between was a mixed state.
async fn acme_only_ever_resolves_to(resolver: &RuntimeResolver, allowed: &[&str], until: &str) {
    let start = Instant::now();
    loop {
        let observed = resolves_to(resolver, "acme");
        match observed.as_deref() {
            Ok(id) if id == until => return,
            Ok(id) if allowed.contains(&id) => {}
            other => panic!("acme resolved to {other:?} between two consistent states"),
        }
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "acme never reached {until}"
        );
        tokio::task::yield_now().await;
    }
}

/// The files the tenants `ConfigMap` projects into one volume, in one
/// kubelet swap: the payload, its manifest, and the publication record.
const TENANTS_VOLUME: &[&str] = &["tenants.json", "tenants.manifest.json", PUBLICATION_RECORD_FILE];

/// The files the data-sources `ConfigMap` projects into its own volume.
const DATA_SOURCES_VOLUME: &[&str] = &["data-sources.json", "data-sources.manifest.json"];

fn second_data_source_is_loaded(resolver: &RuntimeResolver) -> bool {
    let second = DataSourceId::try_new(SECOND_DATA_SOURCE).unwrap();
    resolver.data_sources().lookup(&second).is_ok()
}

/// Asserts the reader is serving the base generation, whole: acme on the
/// first DataSource, which answers requests, and the second DataSource not
/// loaded.
async fn serves_the_base_generation(stack: &support::Stack) {
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );
    assert!(!second_data_source_is_loaded(&stack.resolver));
    let response = stack
        .app
        .clone()
        .oneshot(support::request(
            "GET",
            "/articles/1",
            &support::claims_for("acme"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// Asks for a refresh, and waits until the reader has finished reading.
async fn refresh(stack: &support::Stack) {
    let before = stack.loads.load(Ordering::SeqCst);
    stack.handles.refresh_now();
    support::poll_for_load_count_above(&stack.loads, before, Duration::from_secs(2)).await;
}

/// Gap G1a. The publisher writes data sources before tenants, and the
/// reader applies a complete generation data sources first. A publication
/// that adds a DataSource and moves acme onto it in one snapshot is applied
/// without acme ever resolving to `MissingDataSource`: every sample is the
/// old DataSource or the new one.
#[tokio::test]
async fn a_reader_applies_new_data_sources_before_the_tenants_that_name_them() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    serves_the_base_generation(&stack).await;

    let report = stack
        .dir
        .publisher()
        .publish(&acme_moves_to_second_data_source())
        .await
        .unwrap();
    assert_eq!(report.data_sources, DocumentOutcome::Written);
    assert_eq!(report.tenants, DocumentOutcome::Written);

    stack.handles.refresh_now();
    acme_only_ever_resolves_to(&stack.resolver, &[support::DATA_SOURCE_ID], SECOND_DATA_SOURCE).await;

    let response = stack
        .app
        .clone()
        .oneshot(support::request(
            "GET",
            "/articles/1",
            &support::claims_for("acme"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// Gap G1b, fixed. In Kubernetes each document is its own volume, and the
/// kubelet projects each on its own. Here the tenants volume, record
/// included, is projected and the data-sources volume is not. The
/// data-sources file is not the one the record names, so the reader keeps
/// generation 1 whole:
/// acme still resolves and answers, where the per-document reader served
/// it a 500. Once the data-sources volume arrives, generation 2 applies.
#[tokio::test]
async fn a_reader_holds_its_generation_while_tenants_reach_it_before_the_data_sources_they_name() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    let unprojected = stack.dir.capture(DATA_SOURCES_VOLUME);

    stack
        .dir
        .publisher()
        .publish(&acme_moves_to_second_data_source())
        .await
        .unwrap();
    let projected = stack.dir.capture(DATA_SOURCES_VOLUME);
    stack.dir.restore(&unprojected);

    refresh(&stack).await;
    serves_the_base_generation(&stack).await;

    stack.dir.restore(&projected);
    stack.handles.refresh_now();
    poll_until("generation 2", Duration::from_secs(2), || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;
}

/// Gap G1b, fixed, the other way round: the data-sources volume is
/// projected first. The record in the still-unprojected tenants volume names
/// the previous data-sources bytes, so the new DataSource is not applied
/// ahead of the generation that introduced it.
#[tokio::test]
async fn a_reader_holds_its_generation_while_data_sources_reach_it_before_the_tenants() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    let unprojected = stack.dir.capture(TENANTS_VOLUME);

    stack
        .dir
        .publisher()
        .publish(&acme_moves_to_second_data_source())
        .await
        .unwrap();
    let projected = stack.dir.capture(TENANTS_VOLUME);
    stack.dir.restore(&unprojected);

    refresh(&stack).await;
    serves_the_base_generation(&stack).await;

    stack.dir.restore(&projected);
    stack.handles.refresh_now();
    poll_until("generation 2", Duration::from_secs(2), || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;
}

/// Gap G2, fixed at the reader. Retiring a DataSource takes two
/// publications: one that unbinds acme, then one that drops the
/// DataSource. Both are published back to back, and the reader's tenants
/// volume is still on the generation before either. The data-sources
/// document it reads is not the one that record names, so the reader keeps
/// acme on the DataSource it is still bound to -- which it still holds --
/// rather than applying the retirement first. When the tenants volume
/// arrives, the reader jumps to the generation with both changes.
#[tokio::test]
async fn a_reader_never_applies_a_retirement_before_the_unbinding_that_preceded_it() {
    let stack = support::build_stack(&acme_moves_to_second_data_source()).await;
    assert_eq!(resolves_to(&stack.resolver, "acme").unwrap(), SECOND_DATA_SOURCE);
    let unprojected = stack.dir.capture(TENANTS_VOLUME);

    let mut unbind = support::base_snapshot(3);
    unbind.tenants.payload = vec![
        tenant_on(
            "acme",
            support::ACME_DISCRIMINATOR_VALUE,
            support::DATA_SOURCE_ID,
            3,
        ),
        tenant_on(
            "globex",
            support::GLOBEX_DISCRIMINATOR_VALUE,
            support::DATA_SOURCE_ID,
            1,
        ),
    ];
    unbind.data_sources = DocumentInput::new(
        DocumentRevision::new(2),
        vec![support::shared_data_source(1), second_data_source(1)],
    );
    unbind.catalog = DocumentInput::new(DocumentRevision::new(1), support::articles_catalog());
    stack.dir.publisher().publish(&unbind).await.unwrap();

    let mut retire = unbind.clone();
    retire.data_sources = DocumentInput::new(DocumentRevision::new(3), vec![support::shared_data_source(1)]);
    let report = stack.dir.publisher().publish(&retire).await.unwrap();
    assert_eq!(report.data_sources, DocumentOutcome::Written);
    assert_eq!(report.tenants, DocumentOutcome::Unchanged);
    let projected = stack.dir.capture(TENANTS_VOLUME);
    stack.dir.restore(&unprojected);

    refresh(&stack).await;
    assert_eq!(resolves_to(&stack.resolver, "acme").unwrap(), SECOND_DATA_SOURCE);
    assert!(second_data_source_is_loaded(&stack.resolver));

    stack.dir.restore(&projected);
    stack.handles.refresh_now();
    poll_until("the generation that retired it", Duration::from_secs(2), || {
        !second_data_source_is_loaded(&stack.resolver)
    })
    .await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );
}

/// Each point at which the filesystem adapter can fail part-way through a
/// publication, named by the staging file whose path is blocked to make
/// that one write fail: before anything, after the data sources, after the
/// tenants payload but before its manifest, and after every document but
/// before the record. Every one of them leaves the record naming generation
/// 1, so every one is the same case to a reader.
const FAILURE_POINTS: &[(&str, DocumentKind)] = &[
    (".data-sources.json.tmp", DocumentKind::DataSources),
    (".catalog.json.tmp", DocumentKind::Catalog),
    (".tenants.json.tmp", DocumentKind::Tenants),
    (".tenants.manifest.json.tmp", DocumentKind::Tenants),
    (".publication.json.tmp", DocumentKind::Tenants),
];

/// Gap G3, fixed for readers: an interrupted publication cannot expose
/// mixed revisions. For every failure point, the publication fails
/// `Unwritable` with whatever documents it already wrote left in place, and
/// a reader that refreshes then still serves generation 1 whole. The next
/// publication of the same snapshot completes generation 2, which the
/// reader then applies whole. Throughout, a sampler checks every resolution
/// of acme is one generation's answer, never `MissingDataSource`.
#[tokio::test]
async fn an_interrupted_publication_leaves_readers_on_the_previous_complete_generation() {
    // The catalogue moves to revision 2 as well, so every document is
    // written and every failure point is reached.
    let mut next = acme_moves_to_second_data_source();
    next.catalog = DocumentInput::new(DocumentRevision::new(2), support::articles_catalog());

    for (blocked, document) in FAILURE_POINTS {
        let stack = support::build_stack(&support::base_snapshot(1)).await;
        let sampler = Sampler::start(Arc::clone(&stack.resolver));
        std::fs::create_dir(stack.dir.path().join(blocked)).unwrap();

        let error = stack.dir.publisher().publish(&next).await.unwrap_err();
        assert!(
            matches!(&error, PublicationError::Unwritable { document: failed, .. } if failed == document),
            "{blocked}: {error}"
        );

        refresh(&stack).await;
        serves_the_base_generation(&stack).await;

        std::fs::remove_dir(stack.dir.path().join(blocked)).unwrap();
        stack.dir.publisher().publish(&next).await.unwrap();
        stack.handles.refresh_now();
        poll_until("generation 2", Duration::from_secs(2), || {
            resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
        })
        .await;
        assert!(second_data_source_is_loaded(&stack.resolver));

        sampler.stop_and_check(blocked).await;
    }
}

/// The documents an interrupted publication left behind are still at mixed
/// revisions in the published set -- `current()` reports them -- even though
/// no reader applies them. Closing that in the objects themselves needs one
/// transaction across three documents, which neither a filesystem nor the
/// Kubernetes API has; the record is what makes it invisible to readers.
#[tokio::test]
async fn current_behaviour_an_interrupted_publication_leaves_the_published_set_at_mixed_revisions() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    std::fs::create_dir(stack.dir.path().join(".tenants.json.tmp")).unwrap();

    stack
        .dir
        .publisher()
        .publish(&acme_moves_to_second_data_source())
        .await
        .unwrap_err();

    assert_eq!(
        stack.dir.publisher().current().await.unwrap(),
        PublishedRevisions {
            tenants: Some(DocumentRevision::new(1)),
            data_sources: Some(DocumentRevision::new(2)),
            catalog: Some(DocumentRevision::new(1)),
            registry: None,
        }
    );
    let record: serde_json::Value =
        serde_json::from_slice(&stack.dir.read_raw(PUBLICATION_RECORD_FILE)).unwrap();
    assert_eq!(record["generation"], 1);
    assert_eq!(record["documents"]["data-sources"]["revision"], 1);
}

/// Samples acme's resolution on its own task until stopped, recording
/// whether any sample was something other than one generation's answer.
struct Sampler {
    stop: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<Vec<String>>,
}

impl Sampler {
    fn start(resolver: Arc<RuntimeResolver>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let task = tokio::spawn(async move {
            let mut wrong = Vec::new();
            while !stopping.load(Ordering::SeqCst) {
                match resolves_to(&resolver, "acme").as_deref() {
                    Ok(id) if id == support::DATA_SOURCE_ID || id == SECOND_DATA_SOURCE => {}
                    other => wrong.push(format!("{other:?}")),
                }
                tokio::task::yield_now().await;
            }
            wrong
        });
        Self { stop, task }
    }

    async fn stop_and_check(self, case: &str) {
        self.stop.store(true, Ordering::SeqCst);
        let wrong = self.task.await.unwrap();
        assert!(wrong.is_empty(), "{case}: acme resolved to {wrong:?}");
    }
}

/// Gap G5 (last known good across a restart), with no last-known-good
/// directory configured, which is the default. A running reader that is
/// handed a torn document keeps serving its last complete generation, but
/// that generation lives only in memory: a reader started afterwards reads
/// the files as they are and has nothing older to fall back to. With a
/// directory configured it does; see `last_known_good_restart.rs`. The
/// publisher keeps no last-known-good copy either, and it refuses to publish
/// over a held document it cannot parse even at a newer revision
/// (`plan.rs`), so recovering needs an operator to repair or remove the
/// held file first.
#[tokio::test]
async fn current_behaviour_last_known_good_does_not_survive_a_reader_restart() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    stack.dir.write_raw("data-sources.json", b"{ torn");

    refresh(&stack).await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );

    let restarted = support::build_reader(&stack.dir, support::Reader::Generation).await;
    assert!(restarted.is_err());

    let refused = stack.dir.publisher().publish(&support::base_snapshot(2)).await;
    assert!(
        matches!(refused, Err(PublicationError::Unreadable { .. })),
        "{refused:?}"
    );
}
