//! A Kubernetes restore replaces one object at the version it was read at,
//! keeps the keys it carries beside the document, and writes nothing when
//! refused or when the record it relied on moved.
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
    fn before(&self, _plan: &RestorePlan) -> std::io::Result<()> {
        self.0.lock().unwrap().push("before");
        Ok(())
    }

    fn failed(&self, _plan: &RestorePlan, _error: &PublicationError) {
        self.0.lock().unwrap().push("failed");
    }
}

#[tokio::test]
async fn a_torn_tenants_object_is_replaced_at_its_version_keeping_the_record_and_registry() {
    let (client, _token, task) = fake(vec![
        (200, tenants("10", TORN)),
        (200, data_sources(GOOD)),
        (200, catalog()),
        (200, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);
    let audit = Audit::default();

    let plan = adapter
        .restore(DocumentKind::Tenants, GOOD.as_bytes(), &audit)
        .await
        .unwrap();

    assert_eq!(plan.revision, DocumentRevision::new(4));
    assert_eq!(*audit.0.lock().unwrap(), ["before"]);
    let seen = task.join().unwrap();
    assert_eq!(
        seen[3].line,
        "PUT /api/v1/namespaces/platform-system/configmaps/fabric-runtime-tenants HTTP/1.1"
    );
    let written: serde_json::Value = serde_json::from_str(&seen[3].body).unwrap();
    assert_eq!(written["metadata"]["resourceVersion"], "10");
    assert_eq!(
        written["metadata"]["labels"]["app.kubernetes.io/managed-by"],
        "saas-fabric"
    );
    assert_eq!(written["data"][TENANTS_FILE], GOOD);
    assert_eq!(
        written["data"][TENANTS_MANIFEST_FILE],
        manifest(DocumentKind::Tenants)
    );
    assert_eq!(written["data"][PUBLICATION_RECORD_FILE], record());
    assert_eq!(written["data"][ISSUER_REGISTRY_FILE], REGISTRY);
}

#[tokio::test]
async fn a_data_sources_restore_rereads_the_record_before_replacing_its_object() {
    let (client, _token, task) = fake(vec![
        (200, tenants("10", GOOD)),
        (200, data_sources(TORN)),
        (200, catalog()),
        (200, tenants("10", GOOD)),
        (200, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    adapter
        .restore(DocumentKind::DataSources, GOOD.as_bytes(), &Audit::default())
        .await
        .unwrap();

    let seen = task.join().unwrap();
    assert_eq!(
        seen[4].line,
        "PUT /api/v1/namespaces/platform-system/configmaps/fabric-runtime-data-sources HTTP/1.1"
    );
    let written: serde_json::Value = serde_json::from_str(&seen[4].body).unwrap();
    assert_eq!(written["metadata"]["resourceVersion"], "20");
    assert_eq!(written["data"][DATA_SOURCES_FILE], GOOD);
}

#[tokio::test]
async fn a_data_sources_restore_whose_record_moved_writes_and_audits_nothing() {
    let (client, _token, task) = fake(vec![
        (200, tenants("10", GOOD)),
        (200, data_sources(TORN)),
        (200, catalog()),
        (200, tenants("11", GOOD)),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);
    let audit = Audit::default();

    let error = adapter
        .restore(DocumentKind::DataSources, GOOD.as_bytes(), &audit)
        .await
        .unwrap_err();

    assert!(
        matches!(error, RecoveryError::Target(PublicationError::Unwritable { .. })),
        "{error:?}"
    );
    assert!(audit.0.lock().unwrap().is_empty());
    assert_eq!(task.join().unwrap().len(), 4, "no write was sent");
}

#[tokio::test]
async fn bytes_the_record_does_not_name_are_refused_with_only_reads_sent() {
    let (client, _token, task) = fake(vec![
        (200, tenants("10", TORN)),
        (200, data_sources(GOOD)),
        (200, catalog()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);
    let audit = Audit::default();

    let error = adapter
        .restore(DocumentKind::Tenants, b"[ ]", &audit)
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            RecoveryError::Refused(RestoreRefusal::DigestMismatch { .. })
        ),
        "{error:?}"
    );
    assert!(audit.0.lock().unwrap().is_empty());
    let seen = task.join().unwrap();
    assert!(seen.iter().all(|request| request.line.starts_with("GET ")));
}

#[tokio::test]
async fn a_replace_the_cluster_refuses_as_a_conflict_is_audited_as_failed() {
    let (client, _token, task) = fake(vec![
        (200, tenants("10", TORN)),
        (200, data_sources(GOOD)),
        (200, catalog()),
        (409, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);
    let audit = Audit::default();

    let error = adapter
        .restore(DocumentKind::Tenants, GOOD.as_bytes(), &audit)
        .await
        .unwrap_err();

    assert!(
        matches!(error, RecoveryError::Target(PublicationError::Unwritable { .. })),
        "{error:?}"
    );
    assert_eq!(*audit.0.lock().unwrap(), ["before", "failed"]);
    task.join().unwrap();
}

#[tokio::test]
async fn observing_reads_each_object_once_and_writes_nothing() {
    let (client, _token, task) = fake(vec![
        (200, tenants("10", TORN)),
        (200, data_sources(GOOD)),
        (404, "{}".into()),
    ]);
    let adapter = KubernetesRuntimePublication::with_client(client, NS);

    let observation = adapter.observe().await.unwrap();

    assert_eq!(observation.tenants.payload.as_deref(), Some(TORN.as_bytes()));
    assert_eq!(observation.record.as_deref(), Some(record().as_bytes()));
    assert_eq!(observation.catalog, ObservedDocument::default());
    assert_eq!(task.join().unwrap().len(), 3);
}
