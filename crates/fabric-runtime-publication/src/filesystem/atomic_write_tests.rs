//! Two writers staging the same document through `atomic_write`.
//!
//! `atomic_write` stages every write of a target through one fixed sibling
//! path (`sibling_temp_path`). It used to open it with a truncating create, so
//! two overlapping writers of one document shared one temporary inode and
//! could publish an empty or interleaved, non-JSON document (gap G4a in
//! `docs/roadmap/m2-publication-gap-report.md`). It now opens it with
//! `create_new`, so the second writer is refused and touches nothing. These
//! tests interleave the two writers deterministically: writer A's steps are
//! the literal statements of `atomic_write`, run one at a time, and writer B
//! is the real `atomic_write` or its `create_staging` step. No
//! sleeps and no threads, so the interleaving is the same on every run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{
    atomic_write, atomic_write_with, create_staging, fill_and_sync, remove_stale_staging, sibling_temp_path,
};

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fabric-runtime-publication-atomic-write-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn target(&self) -> PathBuf {
        self.0.join("tenants.json")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn parses_as_json(path: &Path) -> bool {
    serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).is_ok()
}

const WRITER_A: &[u8] = b"[\n  \"a\"\n]\n";
const WRITER_B: &[u8] = b"[\n  \"bbbbbbbbbbbbbbbbbbbb\"\n]\n";

fn stage(path: &Path, bytes: &[u8]) {
    fill_and_sync(create_staging(path).unwrap(), bytes).unwrap();
}

fn create_new(path: &Path) -> std::fs::File {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap()
}

/// Writer A opens the staging path, as `create_staging` does; writer B then
/// runs a whole `atomic_write`. B's exclusive create finds A's file there and
/// is refused, so B neither truncates A's staging nor renames it into place.
/// A finishes and publishes its own complete bytes. Before G4a was fixed, B
/// published, A's write landed on top of B's bytes in the published inode,
/// and the document was not JSON.
#[test]
fn a_second_writer_cannot_stage_over_the_first_and_one_whole_document_is_published() {
    let dir = Dir::new();
    let target = dir.target();
    atomic_write(&target, b"[]\n").unwrap();
    let staging = sibling_temp_path(&target);

    let mut writer_a = create_new(&staging);

    let error = atomic_write(&target, WRITER_B).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&target).unwrap(), b"[]\n");

    writer_a.write_all(WRITER_A).unwrap();
    writer_a.sync_all().unwrap();
    std::fs::rename(&staging, &target).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), WRITER_A);
    assert!(parses_as_json(&target));
}

/// Writer A completes its staging; writer B's `create_staging` is refused
/// rather than truncating A's staged bytes, and A's rename publishes them
/// whole. Before G4a was fixed, B's truncating create emptied A's staging and
/// A published an empty file.
#[test]
fn a_second_writer_cannot_truncate_the_first_writers_staged_bytes() {
    let dir = Dir::new();
    let target = dir.target();
    atomic_write(&target, b"[]\n").unwrap();
    let staging = sibling_temp_path(&target);

    stage(&staging, WRITER_A);
    let error = create_staging(&staging).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    std::fs::rename(&staging, &target).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), WRITER_A);
    assert!(parses_as_json(&target));
}

/// A refused writer leaves the other writer's staging file alone: the file
/// it found is not its own to clean up.
#[test]
fn a_refused_writer_does_not_remove_the_staging_file_it_found() {
    let dir = Dir::new();
    let target = dir.target();
    let staging = sibling_temp_path(&target);
    stage(&staging, WRITER_A);

    assert!(atomic_write(&target, WRITER_B).is_err());

    assert_eq!(std::fs::read(&staging).unwrap(), WRITER_A);
}

/// What a crashed writer left at the staging path refuses every later write
/// until it is removed; `remove_stale_staging`, which `publish` calls under
/// the publication lock, removes it.
#[test]
fn a_staging_file_left_by_a_crashed_writer_is_cleared_by_remove_stale_staging() {
    let dir = Dir::new();
    let target = dir.target();
    stage(&sibling_temp_path(&target), WRITER_A);
    assert!(atomic_write(&target, WRITER_B).is_err());

    remove_stale_staging(&target);
    atomic_write(&target, WRITER_B).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), WRITER_B);
}

/// A create that fails for a reason other than `AlreadyExists` -- here
/// `EMFILE`, too many open files -- made nothing, so whatever is at the
/// staging path is another writer's. It used to be removed, because any
/// error but `AlreadyExists` was taken to mean "this call's file". Ownership
/// is now decided by whether the create returned a file.
#[test]
fn a_failed_create_never_removes_a_staging_file_it_did_not_make() {
    let dir = Dir::new();
    let target = dir.target();
    let staging = sibling_temp_path(&target);
    stage(&staging, WRITER_A);

    let error =
        atomic_write_with(&target, WRITER_B, |_| Err(std::io::Error::from_raw_os_error(24))).unwrap_err();

    assert_eq!(error.raw_os_error(), Some(24));
    assert_eq!(std::fs::read(&staging).unwrap(), WRITER_A);
    assert!(!target.exists());
}

/// A failure after this call created its staging file still cleans it up:
/// the rename fails because the target is a non-empty directory, and the
/// staging file this call made is removed.
#[test]
fn a_failure_after_a_successful_create_removes_the_staging_file() {
    let dir = Dir::new();
    let target = dir.target();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("occupied"), b"x").unwrap();
    let staging = sibling_temp_path(&target);

    assert!(atomic_write(&target, WRITER_B).is_err());

    assert!(!staging.exists());
    assert!(target.join("occupied").exists());
}
