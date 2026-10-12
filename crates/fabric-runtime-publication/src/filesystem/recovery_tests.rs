//! A filesystem restore takes the publication lock, clears a crashed
//! writer's staging file, audits before its first write, and writes nothing
//! when the audit cannot be made durable.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::lock::PublicationLock;
use super::paths::DocumentPaths;
use crate::{
    DocumentKind, DocumentManifest, DocumentRevision, FilesystemRuntimePublication, HeldRecovery as _,
    PublicationError, PublicationRecord, RecordedDocument, RecordedDocuments, RecoveryAudit, RecoveryError,
    ResourceHistory, RestorePlan, PUBLICATION_RECORD_FILE, TENANTS_FILE, TENANTS_MANIFEST_FILE,
};

const GOOD: &[u8] = b"[]";
const TORN: &[u8] = b"[{\"tenant\":";

struct Dir(PathBuf);

impl Dir {
    /// A tenants document at revision 2 whose payload is torn, under a
    /// record that names `GOOD` for it.
    fn torn() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fabric-runtime-publication-recovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        let recorded = RecordedDocument::of(DocumentRevision::new(2), GOOD);
        let record = PublicationRecord::new(
            1,
            RecordedDocuments {
                tenants: recorded.clone(),
                data_sources: recorded.clone(),
                catalog: recorded,
            },
            ResourceHistory::default(),
        );
        let manifest = DocumentManifest::new(DocumentKind::Tenants, DocumentRevision::new(2));
        std::fs::write(
            path.join(PUBLICATION_RECORD_FILE),
            record.canonical_json().unwrap(),
        )
        .unwrap();
        std::fs::write(
            path.join(TENANTS_MANIFEST_FILE),
            manifest.canonical_json().unwrap(),
        )
        .unwrap();
        std::fs::write(path.join(TENANTS_FILE), TORN).unwrap();
        Self(path)
    }

    fn adapter(&self) -> FilesystemRuntimePublication {
        FilesystemRuntimePublication::new(
            self.0.join(TENANTS_FILE),
            self.0.join("data-sources.json"),
            self.0.join("catalog.json"),
        )
    }

    fn tenants(&self) -> Vec<u8> {
        std::fs::read(self.0.join(TENANTS_FILE)).unwrap()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Records what it was asked to audit, and when told to, refuses.
#[derive(Default)]
struct Audit {
    refuse: bool,
    seen: Mutex<Vec<String>>,
    tenants_when_audited: Mutex<Option<Vec<u8>>>,
    path: Option<PathBuf>,
}

impl RecoveryAudit for Audit {
    fn before(&self, plan: &RestorePlan) -> std::io::Result<()> {
        if let Some(path) = &self.path {
            *self.tenants_when_audited.lock().unwrap() = Some(std::fs::read(path).unwrap());
        }
        if self.refuse {
            return Err(std::io::Error::other("the audit volume is full"));
        }
        self.seen
            .lock()
            .unwrap()
            .push(format!("before {:?}", plan.document));
        Ok(())
    }

    fn failed(&self, plan: &RestorePlan, _error: &PublicationError) {
        self.seen
            .lock()
            .unwrap()
            .push(format!("failed {:?}", plan.document));
    }
}

#[tokio::test]
async fn a_restore_is_refused_while_a_publication_holds_the_lock_and_writes_nothing() {
    let dir = Dir::torn();
    let adapter = dir.adapter();
    let held = PublicationLock::acquire(&DocumentPaths::new(
        DocumentKind::Tenants,
        dir.0.join(TENANTS_FILE),
    ))
    .unwrap();
    let audit = Audit::default();

    let error = adapter
        .restore(DocumentKind::Tenants, GOOD, &audit)
        .await
        .unwrap_err();

    assert!(
        matches!(error, RecoveryError::Target(PublicationError::Unwritable { .. })),
        "{error:?}"
    );
    assert_eq!(dir.tenants(), TORN);
    assert!(audit.seen.lock().unwrap().is_empty(), "nothing was audited");
    drop(held);

    adapter
        .restore(DocumentKind::Tenants, GOOD, &audit)
        .await
        .unwrap();
    assert_eq!(dir.tenants(), GOOD);
}

#[tokio::test]
async fn the_audit_is_made_before_the_first_write() {
    let dir = Dir::torn();
    let audit = Audit {
        path: Some(dir.0.join(TENANTS_FILE)),
        ..Audit::default()
    };

    dir.adapter()
        .restore(DocumentKind::Tenants, GOOD, &audit)
        .await
        .unwrap();

    assert_eq!(audit.tenants_when_audited.lock().unwrap().as_deref(), Some(TORN));
    assert_eq!(*audit.seen.lock().unwrap(), ["before Tenants"]);
    assert_eq!(dir.tenants(), GOOD);
}

#[tokio::test]
async fn an_audit_that_cannot_be_made_durable_stops_the_restore_with_nothing_written() {
    let dir = Dir::torn();
    let audit = Audit {
        refuse: true,
        ..Audit::default()
    };

    let error = dir
        .adapter()
        .restore(DocumentKind::Tenants, GOOD, &audit)
        .await
        .unwrap_err();

    assert!(matches!(error, RecoveryError::Audit(_)), "{error:?}");
    assert_eq!(dir.tenants(), TORN);
}

#[tokio::test]
async fn a_staging_file_left_by_a_crashed_writer_does_not_block_the_restore() {
    let dir = Dir::torn();
    std::fs::write(dir.0.join(format!(".{TENANTS_FILE}.tmp")), b"left by a crash").unwrap();

    dir.adapter()
        .restore(DocumentKind::Tenants, GOOD, &Audit::default())
        .await
        .unwrap();

    assert_eq!(dir.tenants(), GOOD);
    assert!(!dir.0.join(format!(".{TENANTS_FILE}.tmp")).exists());
}

#[tokio::test]
async fn a_failed_write_after_the_audit_is_audited_as_failed() {
    let dir = Dir::torn();
    // A directory where the staging file must be created: `remove_stale_staging`
    // cannot clear it, and `atomic_write`'s `create_new` fails on it.
    std::fs::create_dir(dir.0.join(format!(".{TENANTS_FILE}.tmp"))).unwrap();
    let audit = Audit::default();

    let error = dir
        .adapter()
        .restore(DocumentKind::Tenants, GOOD, &audit)
        .await
        .unwrap_err();

    assert!(
        matches!(error, RecoveryError::Target(PublicationError::Unwritable { .. })),
        "{error:?}"
    );
    assert_eq!(*audit.seen.lock().unwrap(), ["before Tenants", "failed Tenants"]);
    assert_eq!(dir.tenants(), TORN);
}
