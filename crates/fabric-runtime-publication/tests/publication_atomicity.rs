//! Pins what publication guarantees about atomicity today, and what it does
//! not, for issue #112 and `docs/roadmap/m2-publication-gap-report.md`.
//!
//! Every test here drives the real `FilesystemRuntimePublication` and the
//! real `build_runtime` over the real `JsonFileSource`. Tests whose name
//! starts with `current_behaviour_` assert a behaviour the gap report names
//! as a gap: they pass because the gap exists, and the fix for that gap is
//! expected to change them. They are not ignored, so a change that closes
//! (or widens) a gap is seen in review rather than discovered later.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, DataSourceDocument, DocumentInput, DocumentOutcome,
    DocumentRevision, FieldName, IsolationModelDocument, PublicationError, PublishedRevisions,
    RuntimePublication as _, RuntimeSnapshot, TenantBindingDocument, TenantDataBindingDocument,
    TenantDataBindings,
};
use fabric_tenant_runtime::{
    build_runtime, DataSource as RuntimeDataSource, JsonFileSource, ResolveError, RuntimeConfig,
    RuntimeResolver, TenantRuntimeBinding,
};
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

fn is_missing_data_source(result: &Result<String, ResolveError>) -> bool {
    matches!(result, Err(ResolveError::MissingDataSource { .. }))
}

/// Gap G1 (cross-document ordering at the reader). The publisher writes
/// data sources before tenants, but the runtime reloads each document on its
/// own refresher (`registration.rs:97-100`), and in Kubernetes each document
/// is its own volume that the kubelet refreshes on its own. Nothing makes the
/// reader apply them in the publisher's order. Here only the tenants
/// refresher has run: acme's new binding is served against a DataSource
/// registry that does not have the DataSource it names, and every acme
/// request fails closed with a 500 until the other refresher catches up.
#[tokio::test]
async fn current_behaviour_a_reader_can_apply_new_tenants_before_the_data_sources_they_name() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );

    let report = stack
        .dir
        .publisher()
        .publish(&acme_moves_to_second_data_source())
        .await
        .unwrap();
    assert_eq!(report.data_sources, DocumentOutcome::Written);
    assert_eq!(report.tenants, DocumentOutcome::Written);

    stack.handles.tenants.refresh_now();
    poll_until("the tenants refresh", Duration::from_secs(2), || {
        is_missing_data_source(&resolves_to(&stack.resolver, "acme"))
    })
    .await;

    // Mixed revisions, observed: tenants at document revision 2, data
    // sources still at 1. acme fails closed; globex, unchanged, still serves.
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
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        resolves_to(&stack.resolver, "globex").unwrap(),
        support::DATA_SOURCE_ID
    );

    stack.handles.data_sources.refresh_now();
    poll_until("the data-sources refresh", Duration::from_secs(2), || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;
}

/// Gap G2 (retirement ordering at the reader). The publisher refuses to
/// drop a DataSource in the same publication that unbinds its last tenant,
/// and checks against the *held* tenants document (`validate.rs:113-131`).
/// That orders the two publications on the publisher's side only. Nothing
/// requires the second to wait until readers have applied the first, so a
/// reader whose data-sources refresher runs first applies the retirement
/// while its tenant registry still binds the retired DataSource.
#[tokio::test]
async fn current_behaviour_a_reader_can_apply_a_retirement_before_the_unbinding_that_preceded_it() {
    let stack = support::build_stack(&acme_moves_to_second_data_source()).await;
    assert_eq!(resolves_to(&stack.resolver, "acme").unwrap(), SECOND_DATA_SOURCE);

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

    stack.handles.data_sources.refresh_now();
    poll_until("the data-sources refresh", Duration::from_secs(2), || {
        is_missing_data_source(&resolves_to(&stack.resolver, "acme"))
    })
    .await;

    stack.handles.tenants.refresh_now();
    poll_until("the tenants refresh", Duration::from_secs(2), || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(support::DATA_SOURCE_ID)
    })
    .await;
}

/// Gap G3 (interrupted publication). An I/O failure after the data-sources
/// document lands and before the tenants document does is reported as
/// `Unwritable` (the one refusal that is not all-or-nothing, `errors.rs:19-20`)
/// and leaves the published set at mixed document revisions. A reader that
/// refreshes in that window serves them: the new DataSource is loaded and
/// acme is still on the old binding. Here the order makes the mixed set
/// harmless; the next publication of the same snapshot completes it.
#[tokio::test]
async fn current_behaviour_an_interrupted_publication_leaves_readers_serving_mixed_revisions() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    std::fs::create_dir(stack.dir.path().join(".tenants.json.tmp")).unwrap();

    let error = stack
        .dir
        .publisher()
        .publish(&acme_moves_to_second_data_source())
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            PublicationError::Unwritable {
                document: fabric_runtime_publication::DocumentKind::Tenants,
                ..
            }
        ),
        "{error}"
    );
    assert_eq!(
        stack.dir.publisher().current().await.unwrap(),
        PublishedRevisions {
            tenants: Some(DocumentRevision::new(1)),
            data_sources: Some(DocumentRevision::new(2)),
            catalog: Some(DocumentRevision::new(1)),
        }
    );

    stack.handles.data_sources.refresh_now();
    stack.handles.tenants.refresh_now();
    let second = DataSourceId::try_new(SECOND_DATA_SOURCE).unwrap();
    poll_until("the data-sources refresh", Duration::from_secs(2), || {
        stack.resolver.data_sources().lookup(&second).is_ok()
    })
    .await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );

    std::fs::remove_dir(stack.dir.path().join(".tenants.json.tmp")).unwrap();
    let report = stack
        .dir
        .publisher()
        .publish(&acme_moves_to_second_data_source())
        .await
        .unwrap();
    assert_eq!(report.data_sources, DocumentOutcome::Unchanged);
    assert_eq!(report.tenants, DocumentOutcome::Written);
}

/// Gap G6 (reader compatibility). The consumer's types deny unknown fields,
/// so a producer that adds one (a forward-incompatible shape) is refused by
/// an older reader. A running reader keeps its last good snapshot; a reader
/// started against the same files has no last good snapshot to keep and
/// fails to prime. Nothing on the publisher's side knows which reader
/// versions are running before it emits a new shape.
#[tokio::test]
async fn current_behaviour_a_forward_incompatible_document_keeps_running_readers_and_stops_new_ones() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    let held = std::fs::read_to_string(stack.dir.tenants_path()).unwrap();
    let mut tenants: serde_json::Value = serde_json::from_str(&held).unwrap();
    // acme is left out as well, so a reader that accepted this document
    // would deprovision acme. The assertion below then tells "refused" from
    // "accepted".
    let tenants = serde_json::Value::Array(
        tenants
            .as_array_mut()
            .unwrap()
            .drain(..)
            .filter(|tenant| tenant["tenant"] != "acme")
            .map(|mut tenant| {
                tenant["tier"] = serde_json::json!("gold");
                tenant
            })
            .collect(),
    );
    stack.dir.write_raw(
        "tenants.json",
        serde_json::to_string_pretty(&tenants).unwrap().as_bytes(),
    );

    let loads_before = stack.tenant_loads.load(std::sync::atomic::Ordering::SeqCst);
    stack.handles.tenants.refresh_now();
    support::poll_for_load_count_above(&stack.tenant_loads, loads_before, Duration::from_secs(2)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );

    let fail_fast = build_runtime(
        &RuntimeConfig {
            refresh_interval_seconds: 3600,
            fail_fast_on_prime: true,
        },
        Arc::new(JsonFileSource::<TenantRuntimeBinding>::new(
            stack.dir.tenants_path(),
        )),
        Arc::new(JsonFileSource::<RuntimeDataSource>::new(
            stack.dir.data_sources_path(),
        )),
    )
    .await;
    let Err(message) = fail_fast else {
        panic!("a reader that cannot parse the published tenants must not prime");
    };
    assert!(message.contains("tier"), "{message}");

    let (unprimed, handles) = build_runtime(
        &RuntimeConfig {
            refresh_interval_seconds: 3600,
            fail_fast_on_prime: false,
        },
        Arc::new(JsonFileSource::<TenantRuntimeBinding>::new(
            stack.dir.tenants_path(),
        )),
        Arc::new(JsonFileSource::<RuntimeDataSource>::new(
            stack.dir.data_sources_path(),
        )),
    )
    .await
    .unwrap();
    assert!(!unprimed.is_primed());
    assert!(matches!(
        resolves_to(&unprimed, "acme"),
        Err(ResolveError::RuntimeUnavailable)
    ));
    handles.shutdown().await.unwrap();
}

/// Gap G5 (last known good across a restart). A running reader that is
/// handed an unreadable document keeps serving its last good snapshot, but
/// that snapshot lives only in memory: a reader started afterwards reads the
/// files as they are and has nothing older to fall back to. The publisher
/// keeps no last-known-good copy either, and it refuses to publish over a
/// held document it cannot parse even at a newer revision
/// (`plan.rs:60-69`), so recovering needs an operator to repair or remove
/// the held file first.
#[tokio::test]
async fn current_behaviour_last_known_good_does_not_survive_a_reader_restart() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    stack.dir.write_raw("data-sources.json", b"{ torn");

    let loads_before = stack.data_source_loads.load(std::sync::atomic::Ordering::SeqCst);
    stack.handles.data_sources.refresh_now();
    support::poll_for_load_count_above(&stack.data_source_loads, loads_before, Duration::from_secs(2)).await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );

    let restarted = build_runtime(
        &support::runtime_config(),
        Arc::new(JsonFileSource::<TenantRuntimeBinding>::new(
            stack.dir.tenants_path(),
        )),
        Arc::new(JsonFileSource::<RuntimeDataSource>::new(
            stack.dir.data_sources_path(),
        )),
    )
    .await;
    assert!(restarted.is_err());

    let refused = stack.dir.publisher().publish(&support::base_snapshot(2)).await;
    assert!(
        matches!(refused, Err(PublicationError::Unreadable { .. })),
        "{refused:?}"
    );
}
