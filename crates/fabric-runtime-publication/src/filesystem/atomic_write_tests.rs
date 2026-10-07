//! Two writers staging the same document through `atomic_write`.
//!
//! `atomic_write` stages every write of a target through one fixed sibling
//! path (`sibling_temp_path`), so two overlapping writers of one document share
//! one temporary inode. The rename makes one *completed* staging atomic. It
//! does not stop a second writer truncating or overwriting the first writer's
//! staging before or after that rename. These tests interleave the two writers
//! deterministically: writer A's steps are the literal statements of
//! `write_and_sync` and `atomic_write`, run one at a time, and writer B is the
//! real `atomic_write`. No sleeps and no threads, so the interleaving is the
//! same on every run. Gap G4a in `docs/roadmap/m2-publication-gap-report.md`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{atomic_write, sibling_temp_path, write_and_sync};

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

/// Writer A opens the shared staging path; writer B then runs a whole
/// `atomic_write`, which truncates that same inode, fills it and renames it
/// over the target. Writer A's still-open handle now points at the published
/// file, and its write lands on top of B's bytes. A's own rename then fails,
/// so A reports an error and B reported success, but the published document
/// is A's bytes followed by the tail of B's: not JSON.
#[test]
fn current_behaviour_overlapping_writers_of_one_document_can_publish_interleaved_bytes() {
    let dir = Dir::new();
    let target = dir.target();
    atomic_write(&target, b"[]\n").unwrap();
    let staging = sibling_temp_path(&target);

    let mut writer_a = std::fs::File::create(&staging).unwrap();

    atomic_write(&target, WRITER_B).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), WRITER_B);

    writer_a.write_all(WRITER_A).unwrap();
    writer_a.sync_all().unwrap();
    assert!(std::fs::rename(&staging, &target).is_err());

    let published = std::fs::read(&target).unwrap();
    assert_eq!(&published[..WRITER_A.len()], WRITER_A);
    assert_eq!(published.len(), WRITER_B.len());
    assert!(
        !parses_as_json(&target),
        "{}",
        String::from_utf8_lossy(&published)
    );
}

/// Writer A completes its staging; writer B's `write_and_sync` begins with
/// `File::create` on the same path, which truncates A's staged bytes before B
/// has written any of its own. A's rename then publishes the truncated inode.
/// Until B's write lands, and for good if writer B stops there (a crash or a
/// killed pod), the published document is empty: not JSON.
#[test]
fn current_behaviour_overlapping_writers_of_one_document_can_publish_an_empty_file() {
    let dir = Dir::new();
    let target = dir.target();
    atomic_write(&target, b"[]\n").unwrap();
    let staging = sibling_temp_path(&target);

    write_and_sync(&staging, WRITER_A).unwrap();
    let _writer_b = std::fs::File::create(&staging).unwrap();
    std::fs::rename(&staging, &target).unwrap();

    assert!(std::fs::read(&target).unwrap().is_empty());
    assert!(!parses_as_json(&target));
}
