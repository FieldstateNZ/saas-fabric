//! Inspection reads the three objects with `GET`s only, bracketed by the
//! tenants object, and judges them by the shared rule.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fabric_runtime_publication::{
    plan_publication, CompletenessGap, DocumentKind, DocumentManifest, DocumentRevision, GapKind,
    HeldDocuments, PublicationCompleteness, PublicationPlan, RecordedPart, RuntimePublication as _,
    UnobservedReason, CATALOG_FILE, CATALOG_MANIFEST_FILE, DATA_SOURCES_FILE, DATA_SOURCES_MANIFEST_FILE,
    PUBLICATION_RECORD_FILE, TENANTS_FILE, TENANTS_MANIFEST_FILE,
};

use crate::testing::{fake, object, snapshot};
use crate::KubernetesRuntimePublication;

const NS: &str = "platform-system";

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn manifest(document: DocumentKind) -> String {
    text(
        &DocumentManifest::new(document, DocumentRevision::new(1))
            .canonical_json()
            .unwrap(),
    )
}

fn plan() -> PublicationPlan {
    plan_publication(&snapshot(1), &HeldDocuments::default()).unwrap()
}

fn tenants(plan: &PublicationPlan, version: &str) -> String {
    object(
        "fabric-runtime-tenants",
        version,
        &[
            (TENANTS_FILE, &text(&plan.tenants.bytes)),
            (TENANTS_MANIFEST_FILE, &manifest(DocumentKind::Tenants)),
            (PUBLICATION_RECORD_FILE, &text(&plan.record.bytes)),
        ],
    )
}

fn data_sources(payload: &str) -> String {
    object(
        "fabric-runtime-data-sources",
        "7",
        &[
            (DATA_SOURCES_FILE, payload),
            (DATA_SOURCES_MANIFEST_FILE, &manifest(DocumentKind::DataSources)),
        ],
    )
}

fn catalog(plan: &PublicationPlan) -> String {
    object(
        "fabric-runtime-catalog",
        "8",
        &[
            (CATALOG_FILE, &text(&plan.catalog.bytes)),
            (CATALOG_MANIFEST_FILE, &manifest(DocumentKind::Catalog)),
        ],
    )
}

#[tokio::test]
async fn a_whole_generation_is_complete_and_is_read_with_four_gets() {
    let plan = plan();
    let (client, _token, task) = fake(vec![
        (200, tenants(&plan, "9")),
        (200, data_sources(&text(&plan.data_sources.bytes))),
        (200, catalog(&plan)),
        (200, tenants(&plan, "9")),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    assert_eq!(
        adapter.inspect().await,
        PublicationCompleteness::Complete {
            record: plan.record.stamp
        }
    );
    let lines: Vec<String> = task.join().unwrap().into_iter().map(|seen| seen.line).collect();
    assert_eq!(
        lines,
        vec![
            "GET /api/v1/namespaces/platform-system/configmaps/fabric-runtime-tenants HTTP/1.1",
            "GET /api/v1/namespaces/platform-system/configmaps/fabric-runtime-data-sources HTTP/1.1",
            "GET /api/v1/namespaces/platform-system/configmaps/fabric-runtime-catalog HTTP/1.1",
            "GET /api/v1/namespaces/platform-system/configmaps/fabric-runtime-tenants HTTP/1.1",
        ]
    );
}

#[tokio::test]
async fn an_object_left_by_an_interrupted_publication_is_incomplete() {
    let plan = plan();
    let (client, _token, task) = fake(vec![
        (200, tenants(&plan, "9")),
        (200, data_sources("[]")),
        (200, catalog(&plan)),
        (200, tenants(&plan, "9")),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    assert_eq!(
        adapter.inspect().await,
        PublicationCompleteness::Incomplete {
            record: plan.record.stamp,
            gaps: vec![CompletenessGap {
                part: RecordedPart::DataSources,
                kind: GapKind::DigestMismatch
            }],
        }
    );
    task.join().unwrap();
}

#[tokio::test]
async fn a_tenants_object_that_moved_during_the_inspection_is_unstable() {
    let plan = plan();
    let (client, _token, task) = fake(vec![
        (200, tenants(&plan, "9")),
        (200, data_sources(&text(&plan.data_sources.bytes))),
        (200, catalog(&plan)),
        (200, tenants(&plan, "10")),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    assert_eq!(
        adapter.inspect().await,
        PublicationCompleteness::Unobserved {
            reason: UnobservedReason::Unstable
        }
    );
    task.join().unwrap();
}

#[tokio::test]
async fn an_object_the_cluster_cannot_supply_leaves_it_unobserved() {
    let plan = plan();
    let (client, _token, task) = fake(vec![
        (200, tenants(&plan, "9")),
        (500, "{\"message\":\"internal detail\"}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    assert_eq!(
        adapter.inspect().await,
        PublicationCompleteness::Unobserved {
            reason: UnobservedReason::Unavailable
        }
    );
    assert_eq!(task.join().unwrap().len(), 2);
}

#[tokio::test]
async fn no_objects_are_unpublished() {
    let (client, _token, task) = fake(vec![
        (404, "{}".into()),
        (404, "{}".into()),
        (404, "{}".into()),
        (404, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    assert_eq!(adapter.inspect().await, PublicationCompleteness::Unpublished);
    task.join().unwrap();
}
