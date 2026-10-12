//! `fabric-control-plane-api held-publication` against files the real
//! publisher wrote: inspect, validate and restore, each through the built
//! binary with no configuration, no environment and no network (gap G5,
//! publisher side).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use fabric_runtime_publication::{
    CatalogDocument, DataSourceDocument, DocumentInput, DocumentRevision, FilesystemRuntimePublication,
    PublicationError, RuntimePublication as _, RuntimeSnapshot, TenantBindingDocument,
};
use serde_json::Value;

const TORN: &[u8] = b"[{\"tenant\":";

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fabric-control-plane-held-publication-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("published")).unwrap();
        std::fs::create_dir_all(path.join("ops")).unwrap();
        Self(path)
    }

    fn published(&self, name: &str) -> PathBuf {
        self.0.join("published").join(name)
    }

    fn ops(&self, name: &str) -> PathBuf {
        self.0.join("ops").join(name)
    }

    fn publisher(&self) -> FilesystemRuntimePublication {
        FilesystemRuntimePublication::new(
            self.published("tenants.json"),
            self.published("data-sources.json"),
            self.published("catalog.json"),
        )
    }

    fn target(&self) -> Vec<String> {
        [
            "--tenants",
            "tenants.json",
            "--data-sources",
            "data-sources.json",
            "--catalog",
            "catalog.json",
        ]
        .iter()
        .enumerate()
        .map(|(index, word)| {
            if index % 2 == 0 {
                (*word).to_owned()
            } else {
                self.published(word).display().to_string()
            }
        })
        .collect()
    }

    fn run(&self, action: &str, rest: &[String]) -> (Output, Value) {
        let output = Command::new(env!("CARGO_BIN_EXE_fabric-control-plane-api"))
            .args(["held-publication", action])
            .args(self.target())
            .args(rest)
            .env_clear()
            .current_dir(&self.0)
            .output()
            .unwrap();
        let report = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
        (output, report)
    }

    fn restore(&self, from: &Path, operator: &str) -> (Output, Value) {
        let rest: Vec<String> = [
            "--document",
            "tenants",
            "--from",
            &from.display().to_string(),
            "--audit",
            &self.ops("audit.jsonl").display().to_string(),
            "--operator",
            operator,
            "--reason",
            "torn tenants volume",
        ]
        .iter()
        .map(|word| (*word).to_owned())
        .collect();
        self.run("restore", &rest)
    }
}

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

/// Revision 1 published, its tenants bytes kept apart as a reader's copy
/// would keep them, then the published tenants document torn.
async fn torn() -> Dir {
    let dir = Dir::new();
    dir.publisher().publish(&snapshot(1)).await.unwrap();
    std::fs::copy(dir.published("tenants.json"), dir.ops("tenants.json")).unwrap();
    std::fs::write(dir.published("tenants.json"), TORN).unwrap();
    dir
}

fn files(dir: &Dir) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = std::fs::read_dir(dir.0.join("published"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

#[tokio::test]
async fn inspect_and_validate_report_the_blocking_document_and_write_nothing() {
    let dir = torn().await;
    let before = files(&dir);

    let (inspect, report) = dir.run("inspect", &[]);
    assert_eq!(inspect.status.code(), Some(0), "{inspect:?}");
    assert_eq!(report["action"], "inspect");
    assert!(report["diagnosis"]["tenants"]["blocking"]
        .as_str()
        .unwrap()
        .starts_with("unreadable payload"));
    assert_eq!(report["diagnosis"]["tenants"]["matches_record"], false);
    assert_eq!(report["diagnosis"]["data_sources"]["blocking"], Value::Null);

    let (validate, report) = dir.run("validate", &[]);
    assert_eq!(validate.status.code(), Some(1), "{validate:?}");
    assert_eq!(report["diagnosis"]["blocked"].as_array().unwrap().len(), 1);
    assert_eq!(files(&dir), before);
}

#[tokio::test]
async fn restore_puts_back_the_recorded_bytes_after_auditing_and_publication_resumes() {
    let dir = torn().await;
    let refused = dir.publisher().publish(&snapshot(2)).await.unwrap_err();
    assert!(
        matches!(refused, PublicationError::Unreadable { .. }),
        "{refused:?}"
    );
    let revisions = dir.publisher().current().await.unwrap();

    let (restore, report) = dir.restore(&dir.ops("tenants.json"), "ops@example");

    assert_eq!(restore.status.code(), Some(0), "{restore:?}");
    assert_eq!(report["restored"]["revision"], 1);
    assert_eq!(report["restored"]["writes_payload"], true);
    assert_eq!(
        std::fs::read(dir.published("tenants.json")).unwrap(),
        std::fs::read(dir.ops("tenants.json")).unwrap()
    );
    let audit = std::fs::read_to_string(dir.ops("audit.jsonl")).unwrap();
    let lines: Vec<Value> = audit
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["action"], "restore");
    assert_eq!(lines[0]["operator"], "ops@example");
    assert_eq!(lines[0]["reason"], "torn tenants volume");
    assert_eq!(lines[0]["plan"]["sha256"], report["restored"]["sha256"]);
    assert_eq!(
        dir.publisher().current().await.unwrap(),
        revisions,
        "no revision moved"
    );

    let (validate, _) = dir.run("validate", &[]);
    assert_eq!(validate.status.code(), Some(0), "{validate:?}");
    dir.publisher().publish(&snapshot(2)).await.unwrap();
}

#[tokio::test]
async fn restore_of_bytes_the_record_does_not_name_is_refused_and_writes_and_audits_nothing() {
    let dir = torn().await;
    std::fs::write(dir.ops("other.json"), b"[]").unwrap();
    let before = files(&dir);

    let (restore, report) = dir.restore(&dir.ops("other.json"), "ops");

    assert_eq!(restore.status.code(), Some(1), "{restore:?}");
    assert!(report["refused"].as_str().unwrap().contains("SHA-256"));
    assert_eq!(files(&dir), before);
    assert!(!dir.ops("audit.jsonl").exists());
}

#[tokio::test]
async fn restore_refuses_an_audit_log_beside_the_published_files() {
    let dir = torn().await;
    let before = files(&dir);
    let rest: Vec<String> = [
        "--document",
        "tenants",
        "--from",
        &dir.ops("tenants.json").display().to_string(),
        "--audit",
        &dir.published("audit.jsonl").display().to_string(),
        "--operator",
        "ops",
        "--reason",
        "x",
    ]
    .iter()
    .map(|word| (*word).to_owned())
    .collect();

    let (restore, _) = dir.run("restore", &rest);

    assert_eq!(restore.status.code(), Some(2), "{restore:?}");
    assert_eq!(files(&dir), before);
}

#[tokio::test]
async fn usage_errors_and_actions_that_would_discard_or_reset_exit_two_and_touch_nothing() {
    let dir = torn().await;
    let before = files(&dir);

    for action in ["discard", "reset", "write", "promote"] {
        let (output, _) = dir.run(action, &[]);
        assert_eq!(output.status.code(), Some(2), "{action}: {output:?}");
        assert!(output.stdout.is_empty());
    }
    let (output, _) = dir.run("restore", &["--document".to_owned(), "tenants".to_owned()]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(files(&dir), before);
}
