//! Kubernetes supports inspection only: every restore is refused before any
//! request or intent, and an inspection reports a record that moved while
//! the objects were read as unstable.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Mutex;

use super::*;
use crate::testing::{fake, object};
use fabric_runtime_publication::{
    DocumentManifest, DocumentRevision, ObservedDocument, PublicationRecord, RecordedDocument,
    RecordedDocuments, ResourceHistory, DATA_SOURCES_FILE, DATA_SOURCES_MANIFEST_FILE, ISSUER_REGISTRY_FILE,
    PUBLICATION_RECORD_FILE, TENANTS_FILE, TENANTS_MANIFEST_FILE,
};

const NS: &str = "platform-system";
const GOOD: &str = "[]";
const TORN: &str = "[{\"tenant\":";
const REGISTRY: &str = "{\"generation\":1}";

fn manifest(document: DocumentKind) -> String {
    String::from_utf8(
        DocumentManifest::new(document, DocumentRevision::new(4))
            .canonical_json()
            .unwrap(),
    )
    .unwrap()
}

fn record() -> String {
    let recorded = RecordedDocument::of(DocumentRevision::new(4), GOOD.as_bytes());
    let record = PublicationRecord::new(
        9,
        RecordedDocuments {
            tenants: recorded.clone(),
            data_sources: recorded.clone(),
            catalog: recorded,
        },
        ResourceHistory::default(),
    );
    String::from_utf8(record.canonical_json().unwrap()).unwrap()
}

fn tenants(version: &str, payload: &str) -> String {
    let (manifest, record) = (manifest(DocumentKind::Tenants), record());
    object(
        "fabric-runtime-tenants",
        version,
        &[
            (TENANTS_FILE, payload),
            (TENANTS_MANIFEST_FILE, &manifest),
            (PUBLICATION_RECORD_FILE, &record),
            (ISSUER_REGISTRY_FILE, REGISTRY),
        ],
    )
}

fn data_sources(payload: &str) -> String {
    let manifest = manifest(DocumentKind::DataSources);
    object(
        "fabric-runtime-data-sources",
        "20",
        &[
            (DATA_SOURCES_FILE, payload),
            (DATA_SOURCES_MANIFEST_FILE, &manifest),
        ],
    )
}

fn catalog() -> String {
    object("fabric-runtime-catalog", "30", &[])
}

#[derive(Default)]
struct Audit(Mutex<Vec<&'static str>>);

impl RecoveryAudit for Audit {
    fn intent(&self, _plan: &RestorePlan) -> std::io::Result<()> {
        self.0.lock().unwrap().push("intent");
        Ok(())
    }
}

#[tokio::test]
async fn every_restore_is_refused_before_any_request_or_intent() {
    // Review 5489986163's probe: the record moved from generation 9 to 10
    // during the audit, the data-sources object's version still held, and
    // a stale replace succeeded. With no established ownership no request
    // of any kind is sent now, so no stale write can land.
    for document in [DocumentKind::Tenants, DocumentKind::DataSources] {
        let (client, _token, task) = fake(Vec::new());
        let adapter = KubernetesRuntimePublication::with_client(client, NS);
        let audit = Audit::default();

        let error = adapter
            .restore(document, GOOD.as_bytes(), &audit)
            .await
            .unwrap_err();

        assert!(
            matches!(
                error,
                RecoveryError::Refused(RestoreRefusal::OwnershipUnestablished)
            ),
            "{error:?}"
        );
        assert!(audit.0.lock().unwrap().is_empty());
        assert!(task.join().unwrap().is_empty(), "no request was sent");
    }
}

#[tokio::test]
async fn an_inspection_reads_each_object_brackets_the_record_and_writes_nothing() {
    let (client, _token, task) = fake(vec![
        (200, tenants("10", TORN)),
        (200, data_sources(GOOD)),
        (404, "{}".into()),
        (200, tenants("10", TORN)),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    let held = adapter.observe().await.unwrap();

    assert!(held.stable);
    assert_eq!(held.observation.tenants.payload.as_deref(), Some(TORN.as_bytes()));
    assert_eq!(held.observation.record.as_deref(), Some(record().as_bytes()));
    assert_eq!(held.observation.registry.as_deref(), Some(REGISTRY.as_bytes()));
    assert_eq!(held.observation.catalog, ObservedDocument::default());
    let seen = task.join().unwrap();
    assert!(seen.iter().all(|request| request.line.starts_with("GET ")));
}

#[tokio::test]
async fn a_record_that_moved_while_read_is_unstable_and_never_clear() {
    let (client, _token, task) = fake(vec![
        (200, tenants("10", GOOD)),
        (200, data_sources(GOOD)),
        (200, catalog()),
        (200, tenants("11", GOOD)),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    let held = adapter.observe().await.unwrap();
    let diagnosis = fabric_runtime_publication::diagnose(&held);

    assert!(!held.stable);
    assert!(!diagnosis.clear);
    assert!(diagnosis
        .problems
        .iter()
        .any(|problem| problem.kind == fabric_runtime_publication::ProblemKind::Unstable));
    task.join().unwrap();
}
