//! Reader and publisher compatibility across the publication record, for
//! issue #112 ("compatible readers precede schema activation") and D01-21
//! (the rollback floor).
//!
//! Contract N is the reader and publisher before the publication record
//! existed: `build_runtime` over two independent `JsonFileSource`s, and a
//! publisher that writes the three documents and their manifests only.
//! Contract N+1 is this one: the publisher writes the record last, and the
//! production reader applies only the complete generation it names. A
//! publisher at contract N is stood in for by publishing and then removing
//! the record, which leaves exactly the files it wrote. Tests whose name
//! starts with `current_behaviour_` pin what still needs a follow-up.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    tenants_canonical_json, ConnectionName, ConnectionSelectorDocument, DataSourceDocument, DocumentInput,
    DocumentRevision, FieldName, IsolationModelDocument, PublicationError, PublicationRecord,
    RecordedDocument, RecordedDocuments, ResourceHistory, RuntimePublication as _, RuntimeSnapshot,
    TenantBindingDocument, TenantDataBindingDocument, TenantDataBindings, CONTRACT_VERSION,
    PUBLICATION_RECORD_FILE,
};
use fabric_tenant_runtime::{ResolveError, RuntimeConfig, RuntimeResolver, READER_CONTRACT};
use support::Reader;

const SECOND_DATA_SOURCE: &str = "shared-postgres-02";

fn logical_primary() -> LogicalDataSourceName {
    LogicalDataSourceName::try_new("primary").unwrap()
}

fn resolves_to(resolver: &RuntimeResolver, tenant: &str) -> Result<String, ResolveError> {
    resolver
        .resolve_data_source(&TenantId::try_new(tenant).unwrap(), &logical_primary())
        .map(|resolved| resolved.data_source.id.to_string())
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
        tenant: TenantId::try_new(tenant).unwrap(),
        revision: BindingRevision::new(revision),
        data: TenantDataBindings::try_new(data).unwrap(),
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    }
}

fn second_data_source() -> DataSourceDocument {
    let mut source = support::shared_data_source(1);
    source.id = DataSourceId::try_new(SECOND_DATA_SOURCE).unwrap();
    source.connection = ConnectionSelectorDocument::Named {
        name: ConnectionName::try_new("shared-02").unwrap(),
    };
    source
}

/// Generation 2: `shared-postgres-02` is added and acme moves onto it.
fn acme_moves() -> RuntimeSnapshot {
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
            vec![support::shared_data_source(1), second_data_source()],
        ),
        catalog: DocumentInput::new(DocumentRevision::new(1), support::articles_catalog()),
    }
}

async fn poll_until(what: &str, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(start.elapsed() < Duration::from_secs(2), "{what} did not happen");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn refresh(stack: &support::Stack) {
    let before = stack.loads.load(Ordering::SeqCst);
    stack.handles.refresh_now();
    support::poll_for_load_count_above(&stack.loads, before, Duration::from_secs(2)).await;
}

/// Writes a record over the files in `dir` that names them exactly, with
/// `edit` applied to the record's JSON -- how a test publishes a generation
/// under a contract this publisher does not write.
fn rewrite_record(dir: &support::TempDir, generation: u64, edit: impl FnOnce(&mut serde_json::Value)) {
    let documents = RecordedDocuments {
        tenants: RecordedDocument::of(DocumentRevision::new(generation), &dir.read_raw("tenants.json")),
        data_sources: RecordedDocument::of(
            DocumentRevision::new(generation),
            &dir.read_raw("data-sources.json"),
        ),
        catalog: RecordedDocument::of(DocumentRevision::new(1), &dir.read_raw("catalog.json")),
    };
    let record = PublicationRecord::new(generation, documents, ResourceHistory::default());
    let mut json: serde_json::Value = serde_json::from_slice(&record.canonical_json().unwrap()).unwrap();
    edit(&mut json);
    dir.write_raw(
        PUBLICATION_RECORD_FILE,
        &serde_json::to_vec_pretty(&json).unwrap(),
    );
}

/// A reader at contract N, against a publication at N+1. It never reads the
/// record, and the documents' shape is unchanged, so it primes and follows
/// each new generation exactly as it did before the record existed.
#[tokio::test]
async fn a_reader_at_contract_n_serves_a_publication_at_n_plus_one() {
    let stack = support::build_stack_with(&support::base_snapshot(1), Reader::PerDocument).await;
    assert!(stack.dir.has(PUBLICATION_RECORD_FILE));
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );

    stack.dir.publisher().publish(&acme_moves()).await.unwrap();
    stack.handles.refresh_now();
    poll_until("generation 2", || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;
}

/// A reader at contract N+1, against documents a publisher at contract N
/// wrote: no record. It reads each document on its own, as every reader did
/// then, so an upgraded reader rolls out ahead of the publisher. Once a
/// publisher at N+1 publishes, the record appears and the reader applies
/// whole generations from then on.
#[tokio::test]
async fn a_reader_at_contract_n_plus_one_serves_a_publication_at_n() {
    let dir = support::TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    dir.remove_raw(PUBLICATION_RECORD_FILE);
    let (resolver, handles, loads) = support::build_reader(&dir, Reader::Generation).await.unwrap();
    assert_eq!(resolves_to(&resolver, "acme").unwrap(), support::DATA_SOURCE_ID);

    dir.publisher().publish(&acme_moves()).await.unwrap();
    dir.remove_raw(PUBLICATION_RECORD_FILE);
    handles.refresh_now();
    poll_until("the unrecorded generation 2", || {
        resolves_to(&resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;

    let report = dir.publisher().publish(&acme_moves()).await.unwrap();
    assert_eq!(
        report.tenants,
        fabric_runtime_publication::DocumentOutcome::Unchanged
    );
    assert!(dir.has(PUBLICATION_RECORD_FILE));
    let before = loads.load(Ordering::SeqCst);
    handles.refresh_now();
    support::poll_for_load_count_above(&loads, before, Duration::from_secs(2)).await;
    assert_eq!(resolves_to(&resolver, "acme").unwrap(), SECOND_DATA_SOURCE);
    handles.shutdown().await.unwrap();
}

/// A publication that raises the reader floor -- how a publisher activates a
/// shape only newer readers understand -- is refused by a reader below it:
/// a running reader keeps its last complete generation, and a reader
/// starting now does not prime. Neither misreads the new generation, and
/// the publisher decides when to activate it, after the readers that can
/// read it are rolled out.
#[tokio::test]
async fn a_publication_above_the_reader_floor_keeps_running_readers_on_their_generation_and_stops_new_ones() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    stack.dir.publisher().publish(&acme_moves()).await.unwrap();
    rewrite_record(&stack.dir, 2, |record| {
        record["minimum_reader_contract"] = serde_json::json!(READER_CONTRACT + 1);
    });

    refresh(&stack).await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );

    let Err(message) = support::build_reader(&stack.dir, Reader::Generation).await else {
        panic!("a reader below the floor must not prime");
    };
    assert!(message.contains("requires reader contract"), "{message}");

    let (unprimed, handles, _) = support::build_reader_with(
        &stack.dir,
        Reader::Generation,
        &RuntimeConfig {
            refresh_interval_seconds: 3600,
            fail_fast_on_prime: false,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        resolves_to(&unprimed, "acme"),
        Err(ResolveError::RuntimeUnavailable)
    ));
    handles.shutdown().await.unwrap();
}

/// Gap G6, residual: the floor is only as good as the publisher that raises
/// it. A document whose shape changed -- here a field the reader does not
/// know -- published under an unchanged floor is still refused only by
/// `deny_unknown_fields` on the reader's own types. The running reader keeps
/// its generation and a new one fails to prime, naming the field. Nothing
/// yet fails CI when a shape change ships without the floor bump it needs.
#[tokio::test]
async fn current_behaviour_a_shape_change_published_without_raising_the_floor_is_refused_only_by_unknown_fields(
) {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    let mut tenants: serde_json::Value = serde_json::from_slice(&stack.dir.read_raw("tenants.json")).unwrap();
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
    stack
        .dir
        .write_raw("tenants.json", &serde_json::to_vec_pretty(&tenants).unwrap());
    rewrite_record(&stack.dir, 2, |_| {});

    refresh(&stack).await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );

    let Err(message) = support::build_reader(&stack.dir, Reader::Generation).await else {
        panic!("a reader that cannot parse the published tenants must not prime");
    };
    assert!(message.contains("tier"), "{message}");
}

/// D01-21, the floor for the published state: a publisher whose contract is
/// below the held record's `minimum_publisher_contract` refuses before
/// writing anything, rather than leaving readers a record it does not
/// understand.
#[tokio::test]
async fn a_publisher_below_the_held_records_rollback_floor_refuses_and_writes_nothing() {
    let dir = support::TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    rewrite_record(&dir, 1, |record| {
        record["minimum_publisher_contract"] = serde_json::json!(CONTRACT_VERSION + 1);
        record["documents"]["audit"] = serde_json::json!({ "revision": 1, "sha256": "00" });
    });
    let before = dir.capture(&[
        "tenants.json",
        "data-sources.json",
        "catalog.json",
        PUBLICATION_RECORD_FILE,
    ]);

    let refused = dir.publisher().publish(&acme_moves()).await.unwrap_err();

    assert!(
        matches!(
            refused,
            PublicationError::BelowRollbackFloor { floor, publisher }
                if floor == CONTRACT_VERSION + 1 && publisher == CONTRACT_VERSION
        ),
        "{refused}"
    );
    for (name, bytes) in &before {
        assert_eq!(&dir.read_raw(name), bytes, "{name} changed");
    }
}

/// The reverse of the reader test above, for publishers. A publisher rolled
/// back below contract N+1 writes documents without updating the record it
/// does not know about. Readers then find every later generation
/// incomplete: a running reader holds the last recorded generation (stale,
/// never mixed) and a new reader does not prime, until an operator removes
/// the record. The release floor in `compatibility.yaml` (D01-21) now
/// refuses that rollback in-product, in Platform Management. What remains is
/// this behaviour after a break-glass edit that bypasses it.
#[tokio::test]
async fn current_behaviour_a_publisher_rolled_back_below_the_record_leaves_readers_on_the_last_recorded_generation(
) {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    let generation_one_record = stack.dir.read_raw(PUBLICATION_RECORD_FILE);

    stack.dir.publisher().publish(&acme_moves()).await.unwrap();
    stack
        .dir
        .write_raw(PUBLICATION_RECORD_FILE, &generation_one_record);

    refresh(&stack).await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );
    let Err(message) = support::build_reader(&stack.dir, Reader::Generation).await else {
        panic!("a reader must not prime from a generation its record does not name");
    };
    assert!(message.contains("not the one generation"), "{message}");

    stack.dir.remove_raw(PUBLICATION_RECORD_FILE);
    stack.handles.refresh_now();
    poll_until("the unrecorded generation 2", || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;
}

/// The record the publisher writes is the one the reader checks: its
/// digests are of the canonical bytes on disk, so a publication through the
/// adapter is always read back as one complete generation.
#[tokio::test]
async fn the_record_a_publisher_writes_names_the_bytes_it_wrote() {
    let dir = support::TempDir::new();
    let snapshot = acme_moves();
    dir.publisher().publish(&snapshot).await.unwrap();

    let record = PublicationRecord::parse_held(&dir.read_raw(PUBLICATION_RECORD_FILE)).unwrap();

    assert_eq!(record.generation(), 1);
    assert_eq!(
        record.documents().tenants,
        RecordedDocument::of(
            DocumentRevision::new(2),
            &tenants_canonical_json(&snapshot.tenants.payload).unwrap()
        )
    );
    assert_eq!(
        record.documents().data_sources,
        RecordedDocument::of(DocumentRevision::new(2), &dir.read_raw("data-sources.json"))
    );
}
