//! A filesystem restore takes the existing publication lock before it reads,
//! decides, records its intent, and only then clears a crashed writer's
//! staging file and writes; a refusal touches nothing, and a write that may
//! have landed is reported uncertain.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::lock::PublicationLock;
use super::paths::DocumentPaths;
use crate::{
    DocumentKind, DocumentManifest, DocumentRevision, FilesystemRuntimePublication, HeldRecovery as _,
    PublicationError, PublicationRecord, RecordedDocument, RecordedDocuments, RecoveryAudit, RecoveryError,
    ResourceHistory, RestorePlan, RestoreRefusal, PUBLICATION_RECORD_FILE, TENANTS_FILE,
    TENANTS_MANIFEST_FILE,
};

const GOOD: &[u8] = b"[]";
const TORN: &[u8] = b"[{\"tenant\":";
const LOCK: &str = ".tenants.json.lock";
const PAYLOAD_STAGING: &str = ".tenants.json.tmp";
const MANIFEST_STAGING: &str = ".tenants.manifest.json.tmp";

struct Dir(PathBuf);

impl Dir {
    /// What a publication at revision 2 leaves, its lock file included,
    /// with the tenants payload then torn.
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
        std::fs::write(path.join(LOCK), b"").unwrap();
        Self(path)
    }

    fn adapter(&self) -> FilesystemRuntimePublication {
        FilesystemRuntimePublication::new(
            self.0.join(TENANTS_FILE),
            self.0.join("data-sources.json"),
            self.0.join("catalog.json"),
        )
    }

    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).unwrap()
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Records each intent, what the tenants file held at that moment, and
/// whether the staging file was still there; refuses when told to.
#[derive(Default)]
struct Audit {
    refuse: bool,
    watch: Option<PathBuf>,
    intents: Mutex<Vec<(Vec<u8>, bool)>>,
}

impl RecoveryAudit for Audit {
    fn intent(&self, _plan: &RestorePlan) -> std::io::Result<()> {
        if self.refuse {
            return Err(std::io::Error::other("the audit volume is full"));
        }
        let held = self.watch.as_ref().map(|dir| {
            (
                std::fs::read(dir.join(TENANTS_FILE)).unwrap(),
                dir.join(PAYLOAD_STAGING).exists(),
            )
        });
        self.intents.lock().unwrap().push(held.unwrap_or_default());
        Ok(())
    }
}

#[tokio::test]
async fn a_restore_is_refused_while_a_publication_holds_the_lock_and_records_nothing() {
    let dir = Dir::torn();
    let held = PublicationLock::acquire(&DocumentPaths::new(
        DocumentKind::Tenants,
        dir.0.join(TENANTS_FILE),
    ))
    .unwrap();
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
    assert_eq!(dir.read(TENANTS_FILE), TORN);
    assert!(audit.intents.lock().unwrap().is_empty());
    drop(held);
    dir.adapter()
        .restore(DocumentKind::Tenants, GOOD, &audit)
        .await
        .unwrap();
    assert_eq!(dir.read(TENANTS_FILE), GOOD);
}

#[tokio::test]
async fn where_no_publication_made_the_lock_file_a_restore_creates_nothing() {
    let dir = Dir::torn();
    std::fs::remove_file(dir.0.join(LOCK)).unwrap();
    let before = dir.names();

    let error = dir
        .adapter()
        .restore(DocumentKind::Tenants, GOOD, &Audit::default())
        .await
        .unwrap_err();

    assert!(matches!(error, RecoveryError::Target(_)), "{error:?}");
    assert_eq!(dir.names(), before);
}

#[tokio::test]
async fn a_refused_restore_leaves_a_crashed_writers_staging_file_alone() {
    // Review 5490101140's probe: a restore refused for its digest deleted a
    // pre-existing `.tenants.json.tmp` before it was refused.
    let dir = Dir::torn();
    std::fs::write(dir.0.join(PAYLOAD_STAGING), b"left by a crash").unwrap();
    let before = dir.names();

    let error = dir
        .adapter()
        .restore(DocumentKind::Tenants, b"[ ]", &Audit::default())
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            RecoveryError::Refused(RestoreRefusal::DigestMismatch { .. })
        ),
        "{error:?}"
    );
    assert_eq!(dir.names(), before);
    assert_eq!(dir.read(PAYLOAD_STAGING), b"left by a crash");
}

#[tokio::test]
async fn the_intent_is_recorded_before_staging_is_cleared_or_anything_is_written() {
    let dir = Dir::torn();
    std::fs::write(dir.0.join(PAYLOAD_STAGING), b"left by a crash").unwrap();
    let audit = Audit {
        watch: Some(dir.0.clone()),
        ..Audit::default()
    };

    dir.adapter()
        .restore(DocumentKind::Tenants, GOOD, &audit)
        .await
        .unwrap();

    assert_eq!(*audit.intents.lock().unwrap(), [(TORN.to_vec(), true)]);
    assert_eq!(dir.read(TENANTS_FILE), GOOD);
    assert!(!dir.0.join(PAYLOAD_STAGING).exists());
}

#[tokio::test]
async fn an_intent_that_cannot_be_made_durable_stops_the_restore_with_nothing_touched() {
    let dir = Dir::torn();
    std::fs::write(dir.0.join(PAYLOAD_STAGING), b"left by a crash").unwrap();
    let before = dir.names();
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
    assert_eq!(dir.read(TENANTS_FILE), TORN);
    assert_eq!(dir.names(), before);
}

#[tokio::test]
async fn a_write_that_replaced_nothing_is_failed_not_uncertain() {
    let dir = Dir::torn();
    // A directory where the staging file must go: it cannot be cleared, and
    // `atomic_write`'s `create_new` fails on it before any rename.
    std::fs::create_dir(dir.0.join(PAYLOAD_STAGING)).unwrap();

    let error = dir
        .adapter()
        .restore(DocumentKind::Tenants, GOOD, &Audit::default())
        .await
        .unwrap_err();

    assert!(matches!(error, RecoveryError::Failed(_)), "{error:?}");
    assert_eq!(dir.read(TENANTS_FILE), TORN);
}

#[tokio::test]
async fn a_restore_stopped_between_payload_and_manifest_is_uncertain_and_a_retry_completes_it() {
    let dir = Dir::torn();
    std::fs::write(dir.0.join(TENANTS_MANIFEST_FILE), b"{\"document\":").unwrap();
    std::fs::create_dir(dir.0.join(MANIFEST_STAGING)).unwrap();

    let error = dir
        .adapter()
        .restore(DocumentKind::Tenants, GOOD, &Audit::default())
        .await
        .unwrap_err();

    assert!(matches!(error, RecoveryError::Uncertain(_)), "{error:?}");
    assert_eq!(dir.read(TENANTS_FILE), GOOD, "the payload landed");

    std::fs::remove_dir(dir.0.join(MANIFEST_STAGING)).unwrap();
    let retry = dir
        .adapter()
        .restore(DocumentKind::Tenants, GOOD, &Audit::default())
        .await
        .unwrap();
    assert!(
        retry.payload.is_none(),
        "the payload is already the recorded bytes"
    );
    assert!(retry.manifest.is_some());

    let audit = Audit::default();
    let again = dir
        .adapter()
        .restore(DocumentKind::Tenants, GOOD, &audit)
        .await
        .unwrap();
    assert!(again.already_restored());
    assert!(
        audit.intents.lock().unwrap().is_empty(),
        "a no-op records no intent"
    );
}
