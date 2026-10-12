//! The local audit trail on disk (ADR 0028): the chain survives a restart and
//! concurrent appends, and a stream that was edited, cut short, reordered,
//! deleted or left half-written is refused when the trail is opened.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use fabric_client_model::ClientId;
use fabric_control_plane::{
    AuditError, AuditRecord, AuditStream, AuditTrail, ChainBreak, MAX_AUDIT_ENTRY_BYTES,
};
use fabric_control_plane_api::local_audit_trail::{LocalAuditTrail, LocalAuditTrailError};

/// A directory under the OS temp root, removed on drop however the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn unique(label: &str) -> Self {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn log(&self) -> PathBuf {
        self.0.join("client.acme.jsonl")
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn acme() -> AuditStream {
    AuditStream::Client(ClientId::try_new("acme").unwrap())
}

fn record(n: u64) -> AuditRecord {
    AuditRecord::new(
        acme(),
        "control_plane.audit.client_tombstoned",
        "tombstone_client",
        "operator",
    )
    .with("revision", n)
}

/// A trail in `dir` holding `n` entries of the acme stream, then closed.
async fn written(dir: &Path, n: u64) {
    let trail = LocalAuditTrail::open(dir).unwrap();
    for at in 1..=n {
        trail.append(record(at), at).await.unwrap();
    }
}

/// The log's lines, edited by `edit`, written back.
fn edit_lines(path: &Path, edit: impl FnOnce(&mut Vec<String>)) {
    let mut lines: Vec<String> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    edit(&mut lines);
    std::fs::write(path, lines.join("\n") + "\n").unwrap();
}

/// What opening `dir` refused, as the chain break it found.
fn refused_with(dir: &Path) -> ChainBreak {
    match LocalAuditTrail::open(dir).err().unwrap() {
        LocalAuditTrailError::Stream {
            source: AuditError::Broken { broken, .. },
            ..
        } => broken,
        other => panic!("expected a broken stream, got {other}"),
    }
}

#[tokio::test]
async fn the_chain_survives_a_restart_and_continues_from_its_head() {
    let dir = TempDir::unique("fabric-audit-restart");
    written(&dir, 3).await;

    let reopened = LocalAuditTrail::open(&dir).unwrap();
    let fourth = reopened.append(record(4), 4).await.unwrap();
    let stored = reopened.read(&acme()).await.unwrap();

    assert_eq!(fourth.sequence(), 4);
    assert_eq!(fourth.previous(), stored.entries[2].hash());
    assert_eq!(stored.verify().unwrap(), Some(fourth.head()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_appends_keep_one_unbroken_chain_on_disk() {
    let dir = TempDir::unique("fabric-audit-concurrent");
    let trail = Arc::new(LocalAuditTrail::open(&dir).unwrap());

    let appends: Vec<_> = (1..=32)
        .map(|n| {
            let trail = Arc::clone(&trail);
            tokio::spawn(async move { trail.append(record(n), n).await })
        })
        .collect();
    for append in appends {
        append.await.unwrap().unwrap();
    }
    drop(trail);

    let reopened = LocalAuditTrail::open(&dir).unwrap();
    let stored = reopened.read(&acme()).await.unwrap();
    assert_eq!(stored.entries.len(), 32);
    assert_eq!(stored.verify().unwrap().map(|head| head.sequence), Some(32));
}

#[tokio::test]
async fn an_edited_entry_is_refused_at_open() {
    let dir = TempDir::unique("fabric-audit-tampered");
    written(&dir, 3).await;
    edit_lines(&dir.log(), |lines| {
        lines[1] = lines[1].replace("\"operator\"", "\"someone-else\"");
    });

    assert_eq!(refused_with(&dir), ChainBreak::Tampered { sequence: 2 });
}

#[tokio::test]
async fn a_dropped_entry_is_refused_at_open() {
    let dir = TempDir::unique("fabric-audit-dropped");
    written(&dir, 4).await;
    edit_lines(&dir.log(), |lines| {
        lines.remove(1);
    });

    assert_eq!(
        refused_with(&dir),
        ChainBreak::Missing {
            expected: 2,
            found: 3
        }
    );
}

#[tokio::test]
async fn reordered_entries_are_refused_at_open() {
    let dir = TempDir::unique("fabric-audit-reordered");
    written(&dir, 4).await;
    edit_lines(&dir.log(), |lines| lines.swap(1, 2));

    assert_eq!(
        refused_with(&dir),
        ChainBreak::OutOfOrder {
            expected: 2,
            found: 3
        }
    );
}

#[tokio::test]
async fn a_cut_off_tail_is_refused_at_open() {
    let dir = TempDir::unique("fabric-audit-truncated");
    written(&dir, 4).await;
    edit_lines(&dir.log(), |lines| lines.truncate(2));

    assert!(matches!(
        refused_with(&dir),
        ChainBreak::Truncated { found: 2, .. }
    ));
}

#[tokio::test]
async fn a_deleted_stream_is_refused_at_open() {
    let dir = TempDir::unique("fabric-audit-deleted");
    written(&dir, 2).await;
    std::fs::remove_file(dir.log()).unwrap();

    assert!(matches!(
        refused_with(&dir),
        ChainBreak::Truncated { found: 0, .. }
    ));
}

#[tokio::test]
async fn a_half_written_last_line_is_reported_not_skipped() {
    let dir = TempDir::unique("fabric-audit-torn");
    written(&dir, 2).await;
    let text = std::fs::read_to_string(dir.log()).unwrap();
    std::fs::write(dir.log(), &text[..text.len() - 10]).unwrap();

    let refused = LocalAuditTrail::open(&dir).err().unwrap();
    assert!(
        matches!(
            &refused,
            LocalAuditTrailError::Stream {
                source: AuditError::Malformed { .. },
                ..
            }
        ),
        "{refused}"
    );
}

#[tokio::test]
async fn an_entry_past_its_recorded_head_is_complete_and_the_head_catches_up() {
    let dir = TempDir::unique("fabric-audit-head-behind");
    written(&dir, 2).await;
    let earlier = std::fs::read_to_string(dir.join("client.acme.head")).unwrap();
    LocalAuditTrail::open(&dir)
        .unwrap()
        .append(record(3), 3)
        .await
        .unwrap();
    std::fs::write(dir.join("client.acme.head"), earlier).unwrap();

    let reopened = LocalAuditTrail::open(&dir).unwrap();

    assert_eq!(reopened.append(record(4), 4).await.unwrap().sequence(), 4);
}

#[tokio::test]
async fn an_oversize_record_is_refused_and_nothing_is_written() {
    let dir = TempDir::unique("fabric-audit-oversize");
    written(&dir, 1).await;
    let before = std::fs::read_to_string(dir.log()).unwrap();

    let trail = LocalAuditTrail::open(&dir).unwrap();
    let refused = trail
        .append(record(2).with("note", "x".repeat(MAX_AUDIT_ENTRY_BYTES)), 2)
        .await;

    assert!(matches!(refused, Err(AuditError::EntryTooLarge { .. })));
    assert_eq!(std::fs::read_to_string(dir.log()).unwrap(), before);
}

#[tokio::test]
async fn a_second_process_cannot_open_the_same_trail() {
    let dir = TempDir::unique("fabric-audit-locked");
    let _held = LocalAuditTrail::open(&dir).unwrap();

    assert!(matches!(
        LocalAuditTrail::open(&dir),
        Err(LocalAuditTrailError::AlreadyOpen { .. })
    ));
}

#[tokio::test]
async fn a_file_that_names_no_stream_is_refused() {
    let dir = TempDir::unique("fabric-audit-unrecognised");
    std::fs::write(dir.join("notes.txt"), "hello").unwrap();

    assert!(matches!(
        LocalAuditTrail::open(&dir),
        Err(LocalAuditTrailError::Unrecognised { .. })
    ));
}
