//! Gap G5, publisher side: a held tenants or data-sources document the
//! publisher cannot parse blocks every publication until it is restored
//! from the bytes the held record names. Every test drives the real
//! `FilesystemRuntimePublication`; the restore never moves a revision, and
//! every guard reads what the record attests afterwards.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::sync::{Arc, Mutex};

use fabric_data_api::ResourceCatalog;
use fabric_runtime_publication::{
    diagnose, DocumentKind, DocumentRevision, HeldRecovery as _, PublicationError, RecoveryAudit,
    RecoveryError, RestorePlan, RestoreRefusal, RuntimePublication as _, DATA_SOURCES_FILE, TENANTS_FILE,
    TENANTS_MANIFEST_FILE,
};
use fabric_tenant_runtime::{build_published_runtime, CatalogueCheck, LastKnownGoodStore, PublishedFiles};
use support::TempDir;

const TORN: &[u8] = b"[{\"tenant\":\"acme\",";

#[derive(Default)]
struct Audit(Mutex<Vec<RestorePlan>>);

impl RecoveryAudit for Audit {
    fn before(&self, plan: &RestorePlan) -> std::io::Result<()> {
        self.0.lock().unwrap().push(plan.clone());
        Ok(())
    }

    fn failed(&self, _plan: &RestorePlan, _error: &PublicationError) {}
}

/// Revision 1 published, and its exact tenants and data-sources bytes.
async fn published() -> (TempDir, Vec<u8>, Vec<u8>) {
    let dir = TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    let tenants = dir.read_raw(TENANTS_FILE);
    let data_sources = dir.read_raw(DATA_SOURCES_FILE);
    (dir, tenants, data_sources)
}

async fn blocked(dir: &TempDir) -> Vec<String> {
    diagnose(&dir.publisher().observe().await.unwrap()).blocked
}

#[tokio::test]
async fn a_torn_tenants_document_is_restored_from_the_recorded_bytes_and_publication_resumes() {
    let (dir, tenants, _) = published().await;
    let before = dir.publisher().current().await.unwrap();
    dir.write_raw(TENANTS_FILE, TORN);

    let refused = dir
        .publisher()
        .publish(&support::base_snapshot(2))
        .await
        .unwrap_err();
    assert!(
        matches!(
            refused,
            PublicationError::Unreadable {
                document: DocumentKind::Tenants,
                ..
            }
        ),
        "{refused:?}"
    );
    assert_eq!(blocked(&dir).await.len(), 1);

    let audit = Audit::default();
    let plan = dir
        .publisher()
        .restore(DocumentKind::Tenants, &tenants, &audit)
        .await
        .unwrap();

    assert_eq!(plan.revision, DocumentRevision::new(1));
    assert_eq!(audit.0.lock().unwrap().len(), 1);
    assert_eq!(dir.read_raw(TENANTS_FILE), tenants);
    assert_eq!(
        dir.publisher().current().await.unwrap(),
        before,
        "no revision moved"
    );
    assert!(blocked(&dir).await.is_empty());
    dir.publisher().publish(&support::base_snapshot(2)).await.unwrap();
}

#[tokio::test]
async fn a_lost_tenants_payload_is_restored_under_its_manifest() {
    let (dir, tenants, _) = published().await;
    dir.remove_raw(TENANTS_FILE);

    let refused = dir
        .publisher()
        .publish(&support::base_snapshot(2))
        .await
        .unwrap_err();
    assert!(
        matches!(refused, PublicationError::HeldPayloadLost { .. }),
        "{refused:?}"
    );

    let plan = dir
        .publisher()
        .restore(DocumentKind::Tenants, &tenants, &Audit::default())
        .await
        .unwrap();

    assert!(plan.manifest.is_none(), "the held manifest is kept");
    dir.publisher().publish(&support::base_snapshot(2)).await.unwrap();
}

#[tokio::test]
async fn a_torn_data_sources_document_is_restored_the_same_way() {
    let (dir, _, data_sources) = published().await;
    dir.write_raw(DATA_SOURCES_FILE, b"[");

    dir.publisher()
        .restore(DocumentKind::DataSources, &data_sources, &Audit::default())
        .await
        .unwrap();

    dir.publisher().publish(&support::base_snapshot(2)).await.unwrap();
}

#[tokio::test]
async fn after_a_restore_every_guard_still_reads_the_restored_content() {
    let (dir, tenants, _) = published().await;
    dir.write_raw(TENANTS_FILE, TORN);
    dir.publisher()
        .restore(DocumentKind::Tenants, &tenants, &Audit::default())
        .await
        .unwrap();

    let mut dropped = support::base_snapshot(2);
    dropped
        .tenants
        .payload
        .retain(|tenant| tenant.tenant.as_str() == "acme");
    let refused = dir.publisher().publish(&dropped).await.unwrap_err();
    assert!(
        matches!(refused, PublicationError::TenantRemoved { .. }),
        "{refused:?}"
    );

    let mut stale = support::base_snapshot(2);
    stale.tenants.revision = DocumentRevision::ZERO;
    let refused = dir.publisher().publish(&stale).await.unwrap_err();
    assert!(
        matches!(refused, PublicationError::StaleRevision { .. }),
        "{refused:?}"
    );
}

#[tokio::test]
async fn bytes_the_record_does_not_name_are_refused_and_nothing_is_written_or_audited() {
    let (dir, _, _) = published().await;
    dir.write_raw(TENANTS_FILE, TORN);
    let other = TempDir::new();
    other
        .publisher()
        .publish(&support::base_snapshot(2))
        .await
        .unwrap();
    let audit = Audit::default();

    let error = dir
        .publisher()
        .restore(DocumentKind::Tenants, &other.read_raw(TENANTS_FILE), &audit)
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            RecoveryError::Refused(RestoreRefusal::DigestMismatch { .. })
        ),
        "{error:?}"
    );
    assert_eq!(dir.read_raw(TENANTS_FILE), TORN);
    assert!(audit.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_interrupted_publication_ahead_of_its_record_is_not_restored() {
    let (dir, tenants, _) = published().await;
    let record = dir.read_raw("publication.json");
    dir.publisher().publish(&support::base_snapshot(2)).await.unwrap();
    // Revision 2's documents landed; the record of revision 1 is what is
    // held, as a publication stopped before its record would leave it.
    dir.write_raw("publication.json", &record);
    dir.write_raw(TENANTS_FILE, TORN);
    let manifest = dir.read_raw(TENANTS_MANIFEST_FILE);

    let error = dir
        .publisher()
        .restore(DocumentKind::Tenants, &tenants, &Audit::default())
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            RecoveryError::Refused(RestoreRefusal::ManifestDisagrees { .. })
        ),
        "{error:?}"
    );
    assert_eq!(dir.read_raw(TENANTS_FILE), TORN);
    assert_eq!(dir.read_raw(TENANTS_MANIFEST_FILE), manifest);
}

struct HostCatalogue;

impl CatalogueCheck for HostCatalogue {
    fn check(&self, catalogue: &[u8]) -> Result<(), String> {
        serde_json::from_slice::<ResourceCatalog>(catalogue)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

#[tokio::test]
async fn a_readers_last_known_good_copy_holds_the_bytes_a_restore_needs() {
    let dir = TempDir::new();
    let copies = TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    let store = LastKnownGoodStore::new(copies.path().join("copy"));
    let reader = PublishedFiles::new(dir.tenants_path(), dir.data_sources_path())
        .with_catalogue(dir.catalog_path())
        .with_last_known_good(store.clone(), Arc::new(HostCatalogue));
    let runtime = build_published_runtime(&support::runtime_config(), Arc::new(reader))
        .await
        .unwrap();
    runtime.handles.shutdown().await.unwrap();
    dir.write_raw(TENANTS_FILE, TORN);

    let copy: serde_json::Value = serde_json::from_slice(&std::fs::read(store.path()).unwrap()).unwrap();
    let tenants = copy["tenants"].as_str().unwrap().as_bytes();
    dir.publisher()
        .restore(DocumentKind::Tenants, tenants, &Audit::default())
        .await
        .unwrap();

    dir.publisher().publish(&support::base_snapshot(2)).await.unwrap();
}
