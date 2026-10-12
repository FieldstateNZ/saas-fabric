//! Syncs that fail: a stream's first entry is not appended until its name is
//! durable, a head that does not sync is caught up on the next open, and a
//! new directory's ancestry is synced.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use fabric_control_plane::{AuditError, AuditRecord, AuditStream, AuditTrail};

use super::durability::{Durability, Os};
use super::LocalAuditTrail;

/// Real syncs, recorded, failing while a switch is on.
#[derive(Default)]
struct Faults {
    dir_syncs: Mutex<Vec<PathBuf>>,
    fail_dir: AtomicBool,
    fail_head_file: AtomicBool,
}

struct Faulty(Arc<Faults>);

impl Durability for Faulty {
    fn sync_file(&self, file: &File, path: &Path) -> io::Result<()> {
        let staging = path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'));
        if staging && self.0.fail_head_file.load(Ordering::SeqCst) {
            return Err(io::Error::other("injected head sync failure"));
        }
        Os.sync_file(file, path)
    }

    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        self.0.dir_syncs.lock().unwrap().push(dir.to_path_buf());
        if self.0.fail_dir.load(Ordering::SeqCst) {
            return Err(io::Error::other("injected directory sync failure"));
        }
        Os.sync_dir(dir)
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn unique(label: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn record(n: u64) -> AuditRecord {
    AuditRecord::new(
        AuditStream::Catalogue,
        "control_plane.audit.test",
        "test",
        "operator",
    )
    .with("n", n)
}

fn faulty(dir: &Path) -> (LocalAuditTrail, Arc<Faults>) {
    let faults = Arc::new(Faults::default());
    let trail = LocalAuditTrail::open_with(dir, Box::new(Faulty(Arc::clone(&faults)))).unwrap();
    faults.dir_syncs.lock().unwrap().clear();
    (trail, faults)
}

fn head_on_disk(dir: &Path) -> u64 {
    let text = std::fs::read_to_string(dir.join("catalogue.head")).unwrap();
    serde_json::from_str::<fabric_control_plane::ChainHead>(&text)
        .unwrap()
        .sequence
}

/// Reopens with real syncs, appends once more, and returns the verified head.
async fn reopened_and_extended(dir: &Path, at: u64) -> u64 {
    let trail = LocalAuditTrail::open(dir).unwrap();
    trail.append(record(at), at).await.unwrap();
    let stored = trail.read(&AuditStream::Catalogue).await.unwrap();
    stored.verify().unwrap().unwrap().sequence
}

#[tokio::test]
async fn creating_the_first_stream_syncs_its_directory_before_the_append_returns() {
    let dir = TempDir::unique("audit-first-stream");
    let (trail, faults) = faulty(&dir.0);

    trail.append(record(1), 1).await.unwrap();
    assert_eq!(
        faults.dir_syncs.lock().unwrap().len(),
        2,
        "the new log, then the head"
    );

    trail.append(record(2), 2).await.unwrap();
    assert_eq!(faults.dir_syncs.lock().unwrap().len(), 3, "only the head");
}

#[tokio::test]
async fn a_failed_directory_sync_on_the_first_stream_is_not_an_append() {
    let dir = TempDir::unique("audit-first-stream-unsynced");
    let (trail, faults) = faulty(&dir.0);

    faults.fail_dir.store(true, Ordering::SeqCst);
    let error = trail.append(record(1), 1).await.unwrap_err();
    assert!(matches!(&error, AuditError::Unavailable { detail } if detail.contains("not durably appended")));

    faults.fail_dir.store(false, Ordering::SeqCst);
    assert!(
        trail.append(record(2), 2).await.is_err(),
        "the stream waits for a reopen"
    );
    drop(trail);

    assert_eq!(reopened_and_extended(&dir.0, 3).await, 2);
}

#[tokio::test]
async fn a_head_that_does_not_sync_is_stale_and_caught_up_on_the_next_open() {
    let dir = TempDir::unique("audit-head-unsynced");
    let (trail, faults) = faulty(&dir.0);
    trail.append(record(1), 1).await.unwrap();

    faults.fail_head_file.store(true, Ordering::SeqCst);
    trail.append(record(2), 2).await.unwrap();
    trail.append(record(3), 3).await.unwrap();
    assert_eq!(head_on_disk(&dir.0), 1);
    assert!(
        dir.0.join(".catalogue.head.next").exists(),
        "an interrupted head is left staged"
    );
    drop(trail);

    assert_eq!(reopened_and_extended(&dir.0, 4).await, 4);
    assert_eq!(head_on_disk(&dir.0), 4);
}

#[tokio::test]
async fn a_failed_directory_sync_after_a_head_rename_keeps_the_entry_and_refuses_an_open() {
    let dir = TempDir::unique("audit-head-rename-unsynced");
    let (trail, faults) = faulty(&dir.0);
    trail.append(record(1), 1).await.unwrap();

    faults.fail_dir.store(true, Ordering::SeqCst);
    trail.append(record(2), 2).await.unwrap();
    drop(trail);

    let refused = LocalAuditTrail::open_with(&dir.0, Box::new(Faulty(Arc::clone(&faults))));
    assert!(refused.is_err(), "an open cannot promise names it could not sync");

    assert_eq!(reopened_and_extended(&dir.0, 3).await, 3);
}

#[test]
fn opening_a_new_directory_syncs_the_parent_of_each_component() {
    let root = TempDir::unique("audit-ancestry");
    let dir = root.0.join("a").join("b");
    let faults = Arc::new(Faults::default());

    LocalAuditTrail::open_with(&dir, Box::new(Faulty(Arc::clone(&faults)))).unwrap();

    let synced = faults.dir_syncs.lock().unwrap().clone();
    assert_eq!(synced.first().map(PathBuf::as_path), Some(Path::new("/")));
    assert!(synced.ends_with(&[root.0.clone(), root.0.join("a"), dir]));
}

#[test]
fn a_retry_after_a_failed_ancestry_sync_syncs_the_ancestry_it_already_created() {
    let root = TempDir::unique("audit-ancestry-retry");
    let dir = root.0.join("a").join("b");
    let faults = Arc::new(Faults::default());

    faults.fail_dir.store(true, Ordering::SeqCst);
    assert!(LocalAuditTrail::open_with(&dir, Box::new(Faulty(Arc::clone(&faults)))).is_err());
    assert!(
        dir.exists(),
        "the directories were created before the sync failed"
    );

    faults.fail_dir.store(false, Ordering::SeqCst);
    faults.dir_syncs.lock().unwrap().clear();
    LocalAuditTrail::open_with(&dir, Box::new(Faulty(Arc::clone(&faults)))).unwrap();

    let synced = faults.dir_syncs.lock().unwrap().clone();
    assert!(synced.ends_with(&[root.0.clone(), root.0.join("a"), dir]));
}

#[test]
fn a_relative_directory_syncs_the_working_directory_it_is_named_in() {
    let name = format!("fabric-audit-relative-{}", std::process::id());
    let _cleanup = TempDir(PathBuf::from(&name));
    let faults = Arc::new(Faults::default());

    LocalAuditTrail::open_with(Path::new(&name), Box::new(Faulty(Arc::clone(&faults)))).unwrap();

    let synced = faults.dir_syncs.lock().unwrap().clone();
    assert_eq!(synced, vec![PathBuf::from("."), PathBuf::from(&name)]);
}
