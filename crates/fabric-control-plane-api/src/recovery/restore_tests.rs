//! The source and audit log are checked against the target as it is inside
//! the publication lock, not as it was when the command started.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fabric_runtime_publication::{
    CatalogDocument, DataSourceDocument, DocumentInput, DocumentKind, DocumentRevision,
    FilesystemRuntimePublication, HeldRecovery as _, RecoveryError, RuntimePublication as _, RuntimeSnapshot,
    TenantBindingDocument,
};

use super::super::audit::AuditLog;
use super::super::audit_file::AuditFile;
use super::super::identity::{identities, published_files, read_source};

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn example<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn snapshot(revision: u64) -> RuntimeSnapshot {
    let revision = DocumentRevision::new(revision);
    RuntimeSnapshot {
        tenants: DocumentInput::new(revision, example::<Vec<TenantBindingDocument>>("tenants.json")),
        data_sources: DocumentInput::new(revision, example::<Vec<DataSourceDocument>>("data-sources.json")),
        catalog: DocumentInput::new(revision, example::<CatalogDocument>("catalog.json")),
        registry: None,
    }
}

#[tokio::test]
async fn an_audit_log_linked_to_a_record_published_after_the_first_check_is_refused_inside_the_lock() {
    // Review 5490404649's probe, in order and without sleeps: identities are
    // sampled at revision 1; revision 2 is published, replacing the record's
    // inode; the audit log is hard-linked to the new record; the early check,
    // made against the stale identities, passes. The append must not.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = Dir(std::env::temp_dir().join(format!(
        "fabric-control-plane-restore-race-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let (published, ops) = (dir.0.join("published"), dir.0.join("ops"));
    std::fs::create_dir_all(&published).unwrap();
    std::fs::create_dir_all(&ops).unwrap();
    let paths = [
        published.join("tenants.json"),
        published.join("data-sources.json"),
        published.join("catalog.json"),
    ];
    let adapter = FilesystemRuntimePublication::new(&paths[0], &paths[1], &paths[2]);
    adapter.publish(&snapshot(1)).await.unwrap();
    let files = published_files(&paths[0], &paths[1], &paths[2]);
    let stale = identities(&files);
    // Held open so revision 1's inodes are not reused while the test runs.
    let _pinned: Vec<std::fs::File> = files
        .iter()
        .filter_map(|path| std::fs::File::open(path).ok())
        .collect();

    adapter.publish(&snapshot(2)).await.unwrap();
    std::fs::copy(&paths[0], ops.join("tenants.json")).unwrap();
    std::fs::write(&paths[0], b"[{\"tenant\":").unwrap();
    std::fs::hard_link(published.join("publication.json"), ops.join("audit.jsonl")).unwrap();
    let record = std::fs::read(published.join("publication.json")).unwrap();

    let (offered, source) = read_source(&ops.join("tenants.json"), &stale).unwrap();
    let log = AuditFile::open(&ops.join("audit.jsonl"), std::slice::from_ref(&published), &stale).unwrap();
    let audit = AuditLog {
        operation_id: "restore-race".to_owned(),
        operator: "ops".to_owned(),
        reason: "probe".to_owned(),
        target: "filesystem".to_owned(),
        from: "ops/tenants.json".to_owned(),
        at_unix_seconds: 0,
        log,
        published: files,
        source,
    };

    let error = adapter
        .restore(DocumentKind::Tenants, &offered, &audit)
        .await
        .unwrap_err();

    assert!(matches!(error, RecoveryError::Audit(_)), "{error:?}");
    assert_eq!(std::fs::read(published.join("publication.json")).unwrap(), record);
    assert_eq!(std::fs::read(&paths[0]).unwrap(), b"[{\"tenant\":");
}
