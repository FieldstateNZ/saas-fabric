//! The reader-compatibility matrix (gap report G6), one test per row,
//! against the production reader: `build_published_runtime` over
//! `PublishedFiles`, which applies only the complete generation a
//! publication record names.
//!
//! Each row is a producer change. Each test asserts what the row states for
//! a running reader, which holds a generation, and for a reader starting
//! now, under both `fail_fast_on_prime` settings where they differ. A shape
//! this publisher cannot write is stood in for by editing the published
//! bytes and writing a record that names them, so the reader sees exactly
//! what a publisher with that shape would leave. Tests whose name starts
//! with `current_behaviour_` pin what still needs a follow-up under D01-1.
//!
//! The row "record floor above the reader" is
//! `reader_compatibility.rs::a_publication_above_the_reader_floor_keeps_running_readers_on_their_generation_and_stops_new_ones`.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_data_api::ResourceCatalog;
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, DataSourceDocument, DocumentInput, DocumentRevision,
    FieldName, IsolationModelDocument, RuntimePublication as _, RuntimeSnapshot, TenantBindingDocument,
    TenantDataBindingDocument, TenantDataBindings, PUBLICATION_RECORD_FILE,
};
use fabric_tenant_runtime::{startup_catalogue, ResolveError, RuntimeConfig};
use http::StatusCode;
use serde_json::{json, Value};
use support::{edit_json, poll_until, refresh, resolves_to, rewrite_record, Reader, Stack, TempDir};

const SECOND_DATA_SOURCE: &str = "shared-postgres-02";

fn tenant_on(
    tenant: &str,
    discriminator_value: &str,
    data_source: &str,
    revision: u64,
) -> TenantBindingDocument {
    let mut data = BTreeMap::new();
    data.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
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

/// Generation 2 at this publisher's shape: `shared-postgres-02` is added and
/// both tenants move onto it, so applying it is visible for either tenant.
fn both_move() -> RuntimeSnapshot {
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
            vec![support::shared_data_source(1), second_data_source()],
        ),
        catalog: DocumentInput::new(DocumentRevision::new(1), support::articles_catalog()),
        registry: None,
    }
}

fn unprimed_config() -> RuntimeConfig {
    RuntimeConfig {
        refresh_interval_seconds: 3600,
        fail_fast_on_prime: false,
        ..RuntimeConfig::default()
    }
}

/// The first element of the JSON array `document` whose `key` is `value`.
fn entry<'a>(document: &'a mut Value, key: &str, value: &str) -> &'a mut Value {
    document
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item[key] == value)
        .unwrap()
}

/// One shape change: the document it edits, the edit, and a word the
/// reader's refusal must name.
struct ShapeChange {
    document: &'static str,
    edit: fn(&mut Value),
    named: &'static str,
}

/// Publishes generation 2 as [`both_move`], then applies `change` to its
/// bytes and records them as generation 2 under an unchanged reader floor.
/// Asserts the generation is refused whole:
/// - the running reader keeps generation 1 for both tenants, so neither
///   document applied;
/// - a reader starting now with `fail_fast_on_prime = true` fails to build,
///   naming the change;
/// - one with `false` starts unprimed and answers `RuntimeUnavailable`.
async fn assert_refused_whole(change: &ShapeChange) {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    stack.dir.publisher().publish(&both_move()).await.unwrap();
    edit_json(&stack.dir, change.document, change.edit);
    rewrite_record(&stack.dir, 2, |_| {});

    refresh(&stack).await;
    for tenant in ["acme", "globex"] {
        assert_eq!(
            resolves_to(&stack.resolver, tenant).unwrap(),
            support::DATA_SOURCE_ID,
            "{}: the running reader applied part of a generation it cannot read",
            change.named
        );
    }

    let Err(message) = support::build_reader(&stack.dir, Reader::Generation).await else {
        panic!(
            "{}: a reader that cannot read the generation must not prime",
            change.named
        );
    };
    assert!(message.contains(change.named), "{}: {message}", change.named);

    let (unprimed, handles, _) =
        support::build_reader_with(&stack.dir, Reader::Generation, &unprimed_config())
            .await
            .unwrap();
    assert!(matches!(
        resolves_to(&unprimed, "globex"),
        Err(ResolveError::RuntimeUnavailable)
    ));
    handles.shutdown().await.unwrap();
    stack.handles.shutdown().await.unwrap();
}

/// Row: new revision, same shape. The running reader applies it at its next
/// refresh, and a reader starting now primes from it.
#[tokio::test]
async fn a_new_revision_at_the_same_shape_is_applied_by_a_running_reader_and_primes_a_new_one() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    stack.dir.publisher().publish(&both_move()).await.unwrap();

    stack.handles.refresh_now();
    poll_until("generation 2", || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;
    assert_eq!(
        resolves_to(&stack.resolver, "globex").unwrap(),
        SECOND_DATA_SOURCE
    );

    let (fresh, handles, _) = support::build_reader(&stack.dir, Reader::Generation)
        .await
        .unwrap();
    assert_eq!(resolves_to(&fresh, "acme").unwrap(), SECOND_DATA_SOURCE);
    handles.shutdown().await.unwrap();
    stack.handles.shutdown().await.unwrap();
}

/// Row: a field added to tenants or data sources, at the top level of a
/// resource or inside one of its nested documents, under an unchanged
/// reader floor. Refused only by `deny_unknown_fields` on the reader's own
/// types: the whole generation, both documents, is refused.
#[tokio::test]
async fn current_behaviour_a_field_added_to_tenants_or_data_sources_refuses_the_whole_generation() {
    let changes = [
        ShapeChange {
            document: "tenants.json",
            edit: |tenants| entry(tenants, "tenant", "globex")["tier"] = json!("gold"),
            named: "tier",
        },
        ShapeChange {
            document: "tenants.json",
            edit: |tenants| {
                entry(tenants, "tenant", "globex")["data"]["primary"]["priority"] = json!(1);
            },
            named: "priority",
        },
        ShapeChange {
            document: "data-sources.json",
            edit: |sources| entry(sources, "id", SECOND_DATA_SOURCE)["owner"] = json!("platform"),
            named: "owner",
        },
        ShapeChange {
            document: "data-sources.json",
            edit: |sources| {
                entry(sources, "id", SECOND_DATA_SOURCE)["pool"]["max_lifetime_seconds"] = json!(600);
            },
            named: "max_lifetime_seconds",
        },
    ];
    for change in &changes {
        assert_refused_whole(change).await;
    }
}

/// Row: a required field removed, under an unchanged reader floor. Refused
/// as a missing field, the whole generation.
#[tokio::test]
async fn current_behaviour_a_required_field_removed_refuses_the_whole_generation() {
    let changes = [
        ShapeChange {
            document: "tenants.json",
            edit: |tenants| {
                entry(tenants, "tenant", "globex")
                    .as_object_mut()
                    .unwrap()
                    .remove("revision");
            },
            named: "revision",
        },
        ShapeChange {
            document: "data-sources.json",
            edit: |sources| {
                entry(sources, "id", SECOND_DATA_SOURCE)
                    .as_object_mut()
                    .unwrap()
                    .remove("connector");
            },
            named: "connector",
        },
        ShapeChange {
            document: "data-sources.json",
            edit: |sources| {
                entry(sources, "id", SECOND_DATA_SOURCE)["residency"]
                    .as_object_mut()
                    .unwrap()
                    .remove("region");
            },
            named: "region",
        },
    ];
    for change in &changes {
        assert_refused_whole(change).await;
    }
}

/// Row: a new enum variant -- an isolation kind, a placement class, a
/// connection kind -- under an unchanged reader floor. Refused as an
/// unknown variant, the whole generation.
#[tokio::test]
async fn current_behaviour_a_new_enum_variant_refuses_the_whole_generation() {
    let changes = [
        ShapeChange {
            document: "tenants.json",
            edit: |tenants| {
                entry(tenants, "tenant", "globex")["data"]["primary"]["isolation"] =
                    json!({ "kind": "row_policy", "policy": "tenant_rows" });
            },
            named: "row_policy",
        },
        ShapeChange {
            document: "data-sources.json",
            edit: |sources| entry(sources, "id", SECOND_DATA_SOURCE)["placement"] = json!("archive"),
            named: "archive",
        },
        ShapeChange {
            document: "data-sources.json",
            edit: |sources| {
                entry(sources, "id", SECOND_DATA_SOURCE)["connection"] =
                    json!({ "kind": "vault", "path": "db/02" });
            },
            named: "vault",
        },
    ];
    for change in &changes {
        assert_refused_whole(change).await;
    }
}

/// Row: one resource invalid at a known shape, here a tenant binding with
/// an empty `data` map. A running reader keeps its held copy of that
/// resource and applies the rest. A reader starting now drops it and serves
/// the rest; with nothing left to serve it does not prime.
#[tokio::test]
async fn one_invalid_resource_keeps_its_held_copy_and_the_rest_of_the_generation_applies() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    stack.dir.publisher().publish(&both_move()).await.unwrap();
    edit_json(&stack.dir, "tenants.json", |tenants| {
        entry(tenants, "tenant", "acme")["data"] = json!({});
    });
    rewrite_record(&stack.dir, 2, |_| {});

    stack.handles.refresh_now();
    poll_until("globex in generation 2", || {
        resolves_to(&stack.resolver, "globex").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        support::DATA_SOURCE_ID
    );

    let (fresh, handles, _) = support::build_reader(&stack.dir, Reader::Generation)
        .await
        .unwrap();
    assert_eq!(resolves_to(&fresh, "globex").unwrap(), SECOND_DATA_SOURCE);
    assert!(matches!(
        resolves_to(&fresh, "acme"),
        Err(ResolveError::UnknownTenant(_))
    ));
    handles.shutdown().await.unwrap();

    edit_json(&stack.dir, "tenants.json", |tenants| {
        entry(tenants, "tenant", "globex")["data"] = json!({});
    });
    rewrite_record(&stack.dir, 3, |_| {});
    let Err(message) = support::build_reader(&stack.dir, Reader::Generation).await else {
        panic!("a reader with no valid tenant binding must not prime");
    };
    assert!(message.contains("tenant"), "{message}");
    stack.handles.shutdown().await.unwrap();
}

/// What `fabric-api`'s `startup::catalog::load` does: the catalogue the
/// record names, decoded by the host's own type.
async fn host_catalogue(dir: &TempDir) -> Result<ResourceCatalog, String> {
    let bytes = startup_catalogue(None, dir.tenants_path(), dir.catalog_path())
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

async fn articles_status(stack: &Stack) -> StatusCode {
    use tower::ServiceExt as _;
    stack
        .app
        .clone()
        .oneshot(support::request(
            "GET",
            "/articles",
            &support::claims_for("globex"),
        ))
        .await
        .unwrap()
        .status()
}

/// Row: catalogue shape change. A running replica never re-reads its
/// catalogue: it keeps the one it started with, and its tenants still
/// refresh. A replica starting now fails to start on a catalogue it cannot
/// decode. Making the catalogue refreshable is outside ADR 0018 (G8).
#[tokio::test]
async fn current_behaviour_a_catalogue_shape_change_leaves_running_replicas_on_their_catalogue_and_stops_new_ones(
) {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    assert_eq!(articles_status(&stack).await, StatusCode::OK);
    stack.dir.publisher().publish(&both_move()).await.unwrap();
    edit_json(&stack.dir, "catalog.json", |catalog| {
        catalog["articles"]["page_size"] = json!(50);
    });
    rewrite_record(&stack.dir, 2, |_| {});

    stack.handles.refresh_now();
    poll_until("generation 2", || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;
    assert_eq!(articles_status(&stack).await, StatusCode::OK);

    let refused = host_catalogue(&stack.dir).await.unwrap_err();
    assert!(refused.contains("page_size"), "{refused}");
    stack.handles.shutdown().await.unwrap();
}

/// Row: `contract_version` bumped in every manifest, shape unchanged. No
/// reader reads a manifest; the version handshake is the record's floor
/// (ADR 0018 §9, amended for G6). Both readers serve the generation.
#[tokio::test]
async fn current_behaviour_a_bumped_manifest_contract_version_is_ignored_by_every_reader() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    stack.dir.publisher().publish(&both_move()).await.unwrap();
    for manifest in [
        "tenants.manifest.json",
        "data-sources.manifest.json",
        "catalog.manifest.json",
    ] {
        edit_json(&stack.dir, manifest, |manifest| {
            manifest["contract_version"] = json!(99);
        });
    }

    stack.handles.refresh_now();
    poll_until("generation 2", || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;

    let (fresh, handles, _) = support::build_reader(&stack.dir, Reader::Generation)
        .await
        .unwrap();
    assert_eq!(resolves_to(&fresh, "acme").unwrap(), SECOND_DATA_SOURCE);
    host_catalogue(&stack.dir).await.unwrap();
    handles.shutdown().await.unwrap();
    stack.handles.shutdown().await.unwrap();
}

/// Row: v2 file names beside v1 (ADR 0018 §9). A reader reads only the
/// paths it is configured with, so documents beside them, of any shape,
/// reach neither reader. The publisher cannot write them (§9's dual-key
/// path is not built).
#[tokio::test]
async fn v2_documents_beside_v1_reach_no_reader() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    for (name, body) in [
        ("tenants.v2.json", json!([{ "tenant": "acme", "tier": "gold" }])),
        (
            "data-sources.v2.json",
            json!([{ "id": "x", "owner": "platform" }]),
        ),
        ("catalog.v2.json", json!({ "articles": { "page_size": 50 } })),
    ] {
        stack.dir.write_raw(name, &serde_json::to_vec(&body).unwrap());
    }
    stack.dir.publisher().publish(&both_move()).await.unwrap();

    stack.handles.refresh_now();
    poll_until("generation 2", || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;

    let (fresh, handles, _) = support::build_reader(&stack.dir, Reader::Generation)
        .await
        .unwrap();
    assert_eq!(resolves_to(&fresh, "acme").unwrap(), SECOND_DATA_SOURCE);
    host_catalogue(&stack.dir).await.unwrap();
    handles.shutdown().await.unwrap();
    stack.handles.shutdown().await.unwrap();
}

/// Row: a key added to the publication record, floor unchanged. The record,
/// unlike the documents, does not deny unknown fields: a publisher may add
/// what older readers can ignore, and raises `minimum_reader_contract` only
/// when they may not. Both readers serve the generation.
#[tokio::test]
async fn a_key_added_to_the_record_under_an_unchanged_floor_is_ignored_by_every_reader() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    stack.dir.publisher().publish(&both_move()).await.unwrap();
    edit_json(&stack.dir, PUBLICATION_RECORD_FILE, |record| {
        record["audit"] = json!({ "by": "controller" });
        record["documents"]["audit"] = json!({ "revision": 1, "sha256": "00" });
    });

    stack.handles.refresh_now();
    poll_until("generation 2", || {
        resolves_to(&stack.resolver, "acme").as_deref() == Ok(SECOND_DATA_SOURCE)
    })
    .await;

    let (fresh, handles, _) = support::build_reader(&stack.dir, Reader::Generation)
        .await
        .unwrap();
    assert_eq!(resolves_to(&fresh, "acme").unwrap(), SECOND_DATA_SOURCE);
    host_catalogue(&stack.dir).await.unwrap();
    handles.shutdown().await.unwrap();
    stack.handles.shutdown().await.unwrap();
}
