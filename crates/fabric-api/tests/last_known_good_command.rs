//! `fabric-api last-known-good` against a copy the real reader persisted:
//! inspect, validate and discard, each through the built binary with no
//! configuration, no environment and no network (gap G5).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod examples_support;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use examples_support::example;
use fabric_tenant_runtime::{
    build_published_runtime, LastKnownGoodState, LastKnownGoodStore, PublishedFiles, RuntimeConfig,
    AUDIT_FILE_NAME,
};

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fabric-api-lkg-command-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("published")).unwrap();
        Self(path)
    }

    fn published(&self, name: &str) -> PathBuf {
        self.0.join("published").join(name)
    }

    fn last_known_good(&self) -> PathBuf {
        self.0.join("lkg")
    }

    fn copy(&self) -> PathBuf {
        LastKnownGoodStore::new(self.last_known_good())
            .path()
            .to_path_buf()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Publishes the shipped examples as generation 3, as the publisher lays
/// them out, and returns the reader over them with the copy configured.
fn publish_examples(dir: &Dir) -> PublishedFiles {
    let mut digests = serde_json::Map::new();
    for (file, key) in [
        ("tenants.json", "tenants"),
        ("data-sources.json", "data-sources"),
        ("catalog.json", "catalog"),
    ] {
        let bytes = std::fs::read(example(file)).unwrap();
        std::fs::write(dir.published(file), &bytes).unwrap();
        digests.insert(
            key.to_owned(),
            serde_json::json!({ "revision": 3, "sha256": sha256_hex(&bytes) }),
        );
    }
    let record = serde_json::json!({
        "contract_version": 3,
        "minimum_reader_contract": 1,
        "minimum_publisher_contract": 3,
        "generation": 3,
        "documents": digests,
    });
    std::fs::write(dir.published("publication.json"), record.to_string()).unwrap();
    reader(dir)
}

fn reader(dir: &Dir) -> PublishedFiles {
    PublishedFiles::new(dir.published("tenants.json"), dir.published("data-sources.json"))
        .with_catalogue(dir.published("catalog.json"))
        .with_last_known_good(LastKnownGoodStore::new(dir.last_known_good()))
}

/// Starts the real reader once, which persists generation 3, and stops it.
async fn persist_through_the_reader(dir: &Dir) {
    let runtime = build_published_runtime(&RuntimeConfig::default(), Arc::new(publish_examples(dir)))
        .await
        .unwrap();
    assert_eq!(
        runtime.status.report().last_known_good,
        LastKnownGoodState::Stored { generation: 3 }
    );
    runtime.handles.shutdown().await.unwrap();
}

fn command(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fabric-api"))
        .args(arguments)
        .env_clear()
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap()
}

fn report(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}: {output:?}"))
}

fn directory(dir: &Dir) -> String {
    dir.last_known_good().display().to_string()
}

fn tear(path: &Path) {
    std::fs::write(path, b"{ torn").unwrap();
}

#[tokio::test]
async fn inspect_and_validate_report_a_copy_the_reader_persisted_and_write_nothing() {
    let dir = Dir::new();
    persist_through_the_reader(&dir).await;
    let before = std::fs::read(dir.copy()).unwrap();

    let inspected = command(&["last-known-good", "inspect", "--dir", &directory(&dir)]);
    assert_eq!(inspected.status.code(), Some(0), "{inspected:?}");
    let inspection = &report(&inspected)["inspection"];
    assert_eq!(inspection["verdict"], "valid");
    assert_eq!(inspection["generation"], 3);
    assert_eq!(inspection["stated_generation"], 3);
    assert_eq!(inspection["file_sha256"], sha256_hex(&before));

    let validated = command(&["last-known-good", "validate", "--dir", &directory(&dir)]);
    assert_eq!(validated.status.code(), Some(0), "{validated:?}");

    assert_eq!(std::fs::read(dir.copy()).unwrap(), before);
    assert!(!dir.last_known_good().join(AUDIT_FILE_NAME).exists());
}

#[tokio::test]
async fn validate_fails_on_a_corrupted_copy_and_on_no_copy() {
    let dir = Dir::new();
    persist_through_the_reader(&dir).await;
    let mut bytes = std::fs::read(dir.copy()).unwrap();
    let at = bytes.len() / 2;
    bytes[at] ^= 0x01;
    std::fs::write(dir.copy(), &bytes).unwrap();

    let corrupted = command(&["last-known-good", "validate", "--dir", &directory(&dir)]);
    assert_eq!(corrupted.status.code(), Some(1), "{corrupted:?}");
    assert_eq!(report(&corrupted)["inspection"]["verdict"], "refused");

    let empty = Dir::new();
    let absent = command(&["last-known-good", "validate", "--dir", &directory(&empty)]);
    assert_eq!(absent.status.code(), Some(1), "{absent:?}");
    assert_eq!(report(&absent)["inspection"]["verdict"], "absent");
}

#[tokio::test]
async fn discard_needs_an_operator_and_a_reason_and_touches_nothing_without_them() {
    let dir = Dir::new();
    persist_through_the_reader(&dir).await;

    let refused = command(&["last-known-good", "discard", "--dir", &directory(&dir)]);

    assert_eq!(refused.status.code(), Some(2), "{refused:?}");
    assert!(dir.copy().exists());
    assert!(!dir.last_known_good().join(AUDIT_FILE_NAME).exists());
}

#[tokio::test]
async fn discard_audits_moves_the_copy_aside_and_a_restart_on_a_torn_volume_then_fails_closed() {
    let dir = Dir::new();
    persist_through_the_reader(&dir).await;

    let discarded = command(&[
        "last-known-good",
        "discard",
        "--dir",
        &directory(&dir),
        "--operator",
        "ops@example.test",
        "--reason",
        "incident 42",
    ]);
    assert_eq!(discarded.status.code(), Some(0), "{discarded:?}");
    let moved_to = report(&discarded)["discarded"]["moved_to"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(Path::new(&moved_to).exists());
    assert!(!dir.copy().exists(), "nothing is written in the copy's place");

    let audit = std::fs::read_to_string(dir.last_known_good().join(AUDIT_FILE_NAME)).unwrap();
    let line: serde_json::Value = serde_json::from_str(audit.lines().next().unwrap()).unwrap();
    assert_eq!(line["operator"], "ops@example.test");
    assert_eq!(line["reason"], "incident 42");
    assert_eq!(line["inspection"]["generation"], 3);

    let again = command(&[
        "last-known-good",
        "discard",
        "--dir",
        &directory(&dir),
        "--operator",
        "ops@example.test",
        "--reason",
        "again",
    ]);
    assert_eq!(again.status.code(), Some(1), "{again:?}");

    tear(&dir.published("data-sources.json"));
    let restarted = build_published_runtime(&RuntimeConfig::default(), Arc::new(reader(&dir))).await;
    assert!(
        restarted.is_err(),
        "with the copy discarded, a torn volume fails the prime"
    );
}

#[test]
fn an_unknown_action_is_a_usage_error_and_no_action_can_write_a_copy() {
    for action in ["restore", "write", "promote"] {
        let output = command(&["last-known-good", action, "--dir", "/nonexistent"]);
        assert_eq!(output.status.code(), Some(2), "{action}: {output:?}");
        assert!(output.stdout.is_empty());
    }
}
