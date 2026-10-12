//! Replacing the copy is all-or-nothing, wherever the write stops.
//!
//! Each interruption is injected at a named [`Step`] through the checkpoint
//! `atomic_replace` calls after it, which is the same place an I/O failure
//! or a crash would stop it. A crash that kills the process outright is
//! simulated by leaving a staging file behind, since nothing then runs to
//! remove it.

use std::io;
use std::path::PathBuf;

use super::atomic_replace::{atomic_replace, clear_stale_staging, staging_prefix, Step};
use crate::testing::TempDir;

const OLD: &[u8] = b"{\"generation\":1}";
const NEW: &[u8] = b"{\"generation\":2,\"longer\":true}";

fn target(dir: &TempDir) -> PathBuf {
    let target = dir.path().join("last-known-good.json");
    std::fs::write(&target, OLD).unwrap();
    target
}

fn staging_files(dir: &TempDir) -> Vec<String> {
    let prefix = staging_prefix(&dir.path().join("last-known-good.json"));
    std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(&prefix))
        .collect()
}

fn stop_at(stop: Step) -> impl Fn(Step) -> io::Result<()> {
    move |step| {
        if step == stop {
            Err(io::Error::other(format!("interrupted at {step:?}")))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn a_completed_replace_leaves_the_new_bytes_and_no_staging_file() {
    let dir = TempDir::new("lkg-atomic");
    let target = target(&dir);

    atomic_replace(&target, NEW, |_| Ok(())).await.unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), NEW);
    assert!(staging_files(&dir).is_empty());
}

#[tokio::test]
async fn a_write_stopped_before_the_rename_leaves_the_previous_copy_whole_and_cleans_up() {
    for step in [Step::Written, Step::Synced] {
        let dir = TempDir::new("lkg-atomic");
        let target = target(&dir);

        let stopped = atomic_replace(&target, NEW, stop_at(step)).await;

        assert!(stopped.is_err(), "{step:?}");
        assert_eq!(std::fs::read(&target).unwrap(), OLD, "{step:?}");
        assert!(staging_files(&dir).is_empty(), "{step:?}");
    }
}

#[tokio::test]
async fn a_write_stopped_after_the_rename_has_published_the_whole_new_copy() {
    let dir = TempDir::new("lkg-atomic");
    let target = target(&dir);

    let stopped = atomic_replace(&target, NEW, stop_at(Step::Renamed)).await;

    assert!(stopped.is_err());
    assert_eq!(std::fs::read(&target).unwrap(), NEW);
    assert!(staging_files(&dir).is_empty());
}

#[tokio::test]
async fn a_staging_file_left_by_a_crash_never_reaches_the_copy_and_is_cleared_at_start() {
    let dir = TempDir::new("lkg-atomic");
    let target = target(&dir);
    let crashed = dir.path().join(format!("{}4242.0.tmp", staging_prefix(&target)));
    std::fs::write(&crashed, &NEW[..10]).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), OLD);
    clear_stale_staging(&target).await;
    assert!(staging_files(&dir).is_empty());

    atomic_replace(&target, NEW, |_| Ok(())).await.unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), NEW);
}

#[tokio::test]
async fn clearing_leaves_files_that_are_not_this_targets_staging_alone() {
    let dir = TempDir::new("lkg-atomic");
    let target = target(&dir);
    std::fs::write(dir.path().join("audit.jsonl"), b"{}\n").unwrap();
    std::fs::write(dir.path().join("last-known-good.json.discarded-1-abc"), OLD).unwrap();

    clear_stale_staging(&target).await;

    assert!(dir.path().join("audit.jsonl").exists());
    assert!(dir.path().join("last-known-good.json.discarded-1-abc").exists());
    assert_eq!(std::fs::read(&target).unwrap(), OLD);
}
