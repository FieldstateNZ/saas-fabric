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
use fabric_tenant_runtime::{ResolveError, RuntimeConfig, RuntimeResolver};
use http::StatusCode;
use serde_json::{json, Value};
use support::{edit_json, poll_until, refresh, resolves_to, rewrite_record, Reader, Stack};

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
/// an empty `data` map. A recorded generation is installed whole or not at
/// all (G8), so neither reader takes the rest of it: a running reader holds
/// its whole view, globex included, and a reader starting now does not
/// prime. Before G8 a running reader kept its held copy of the one resource
/// and applied the rest beside the new catalogue.
#[tokio::test]
async fn one_invalid_resource_holds_the_whole_generation_for_running_and_starting_readers() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    stack.dir.publisher().publish(&both_move()).await.unwrap();
    edit_json(&stack.dir, "tenants.json", |tenants| {
        entry(tenants, "tenant", "acme")["data"] = json!({});
    });
    rewrite_record(&stack.dir, 2, |_| {});

    refresh(&stack).await;
    for tenant in ["acme", "globex"] {
        assert_eq!(
            resolves_to(&stack.resolver, tenant).unwrap(),
            support::DATA_SOURCE_ID,
            "{tenant} stays on generation 1"
        );
    }
    assert_eq!(
        stack.resolver.served_generation().map(|report| report.generation),
        Some(1)
    );

    let Err(message) = support::build_reader(&stack.dir, Reader::Generation).await else {
        panic!("a reader must not prime from a generation it cannot take whole");
    };
    assert!(message.contains("tenant"), "{message}");
    stack.handles.shutdown().await.unwrap();
}

/// Whether `resolver` serves a catalogue the Data API can read: the one its
/// installed view carries, since there is no other (G8).
fn serves_a_catalogue(resolver: &RuntimeResolver) -> bool {
    resolver
        .pin()
        .catalogue()
        .and_then(|catalogue| catalogue.get::<ResourceCatalog>())
        .is_some()
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

/// Row: catalogue shape change. A catalogue the host cannot decode is never
/// installed (G8). A running replica holds its whole view -- catalogue,
/// tenants, data sources and stamp -- rather than taking the new tenants
/// with its old catalogue. A replica starting now refuses it under
/// `fail_fast_on_prime`, and otherwise starts unprimed, serving no
/// catalogue and reporting no generation. A catalogue change the host can
/// decode is installed: `catalogue_generation.rs`.
#[tokio::test]
async fn a_catalogue_shape_change_holds_running_replicas_on_their_whole_view_and_stops_new_ones() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;
    assert_eq!(articles_status(&stack).await, StatusCode::OK);
    let served = stack.resolver.served_generation();
    assert!(served.is_some());
    let first = resolves_to(&stack.resolver, "acme").unwrap();
    stack.dir.publisher().publish(&both_move()).await.unwrap();
    edit_json(&stack.dir, "catalog.json", |catalog| {
        catalog["articles"]["page_size"] = json!(50);
    });
    rewrite_record(&stack.dir, 2, |_| {});

    refresh(&stack).await;
    assert_eq!(
        resolves_to(&stack.resolver, "acme").unwrap(),
        first,
        "tenants held too"
    );
    assert_eq!(stack.resolver.served_generation(), served);
    assert_eq!(articles_status(&stack).await, StatusCode::OK);

    let Err(refused) = support::build_reader(&stack.dir, Reader::Generation).await else {
        panic!("a reader starting on a catalogue the host cannot decode must not prime");
    };
    assert!(refused.contains("page_size"), "{refused}");
    let unprimed = RuntimeConfig {
        fail_fast_on_prime: false,
        ..support::runtime_config()
    };
    let (fresh, handles, _) = support::build_reader_with(&stack.dir, Reader::Generation, &unprimed)
        .await
        .unwrap();
    assert!(!serves_a_catalogue(&fresh));
    assert_eq!(fresh.served_generation(), None);
    handles.shutdown().await.unwrap();
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
    assert!(serves_a_catalogue(&fresh));
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
    assert!(serves_a_catalogue(&fresh));
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
    assert!(serves_a_catalogue(&fresh));
    handles.shutdown().await.unwrap();
    stack.handles.shutdown().await.unwrap();
}
