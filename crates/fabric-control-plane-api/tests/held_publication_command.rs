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
        self.restore_auditing(from, &self.ops("audit.jsonl"), operator)
    }

    fn restore_auditing(&self, from: &Path, audit: &Path, operator: &str) -> (Output, Value) {
        let rest: Vec<String> = [
            "--document",
            "tenants",
            "--from",
            &from.display().to_string(),
            "--audit",
            &audit.display().to_string(),
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

fn audit_lines(dir: &Dir) -> Vec<Value> {
    std::fs::read_to_string(dir.ops("audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn inspect_and_validate_report_the_blocking_document_and_write_nothing() {
    let dir = torn().await;
    let before = files(&dir);

    let (inspect, report) = dir.run("inspect", &[]);
    assert_eq!(inspect.status.code(), Some(0), "{inspect:?}");
    assert_eq!(report["diagnosis"]["clear"], false);
    let problems = report["diagnosis"]["problems"].as_array().unwrap();
    assert!(problems
        .iter()
        .any(|problem| problem["part"] == "tenants" && problem["kind"] == "blocks_publication"));
    assert_eq!(report["diagnosis"]["tenants"]["matches_record"], false);

    let (validate, _) = dir.run("validate", &[]);
    assert_eq!(validate.status.code(), Some(1), "{validate:?}");
    assert_eq!(files(&dir), before);
}

#[tokio::test]
async fn restore_records_intent_then_completion_and_publication_resumes() {
    let dir = torn().await;
    let refused = dir.publisher().publish(&snapshot(2)).await.unwrap_err();
    assert!(
        matches!(refused, PublicationError::Unreadable { .. }),
        "{refused:?}"
    );
    let revisions = dir.publisher().current().await.unwrap();
    let record = std::fs::read(dir.published("publication.json")).unwrap();

    let (restore, report) = dir.restore(&dir.ops("tenants.json"), "ops@example");

    assert_eq!(restore.status.code(), Some(0), "{restore:?}");
    assert_eq!(report["restored"]["revision"], 1);
    assert_eq!(
        std::fs::read(dir.published("tenants.json")).unwrap(),
        std::fs::read(dir.ops("tenants.json")).unwrap()
    );
    let lines = audit_lines(&dir);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["action"], "restore_intent");
    assert_eq!(lines[0]["operator_self_identified"], "ops@example");
    assert_eq!(lines[0]["reason"], "torn tenants volume");
    assert_eq!(lines[1]["action"], "restore_completed");
    assert_eq!(lines[0]["operation_id"], report["operation_id"]);
    assert_eq!(lines[1]["operation_id"], report["operation_id"]);
    assert_eq!(
        dir.publisher().current().await.unwrap(),
        revisions,
        "no revision moved"
    );
    assert_eq!(std::fs::read(dir.published("publication.json")).unwrap(), record);

    let (validate, _) = dir.run("validate", &[]);
    assert_eq!(validate.status.code(), Some(0), "{validate:?}");
    dir.publisher().publish(&snapshot(2)).await.unwrap();
}

#[tokio::test]
async fn a_repeated_restore_is_an_idempotent_no_op_that_records_nothing() {
    let dir = torn().await;
    let (first, _) = dir.restore(&dir.ops("tenants.json"), "ops");
    assert_eq!(first.status.code(), Some(0), "{first:?}");

    let (again, report) = dir.restore(&dir.ops("tenants.json"), "ops");

    assert_eq!(again.status.code(), Some(0), "{again:?}");
    assert_eq!(report["already_restored"], true);
    assert_eq!(audit_lines(&dir).len(), 2, "only the first restore's lines");
}

#[tokio::test]
async fn a_refused_restore_has_no_effect_not_even_an_audit_file() {
    let dir = torn().await;
    std::fs::write(dir.ops("other.json"), b"[]").unwrap();
    let before = files(&dir);

    let (restore, report) = dir.restore(&dir.ops("other.json"), "ops");

    assert_eq!(restore.status.code(), Some(1), "{restore:?}");
    assert!(report["refused"].as_str().unwrap().contains("SHA-256"));
    assert_eq!(files(&dir), before);
    assert!(!dir.ops("audit.jsonl").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn an_audit_log_that_resolves_to_a_published_file_or_the_source_has_no_effect() {
    // Review 5489986163's probe: an audit symlink outside the publication
    // directory pointed at publication.json, and the audit line was
    // appended into it. Review 5490101140's: an audit hard link to the
    // source copy, appended into the backup.
    let dir = torn().await;
    let source = std::fs::read(dir.ops("tenants.json")).unwrap();
    std::os::unix::fs::symlink(dir.published("publication.json"), dir.ops("symlinked.jsonl")).unwrap();
    std::fs::hard_link(dir.published("publication.json"), dir.ops("record-link.jsonl")).unwrap();
    std::fs::hard_link(dir.ops("tenants.json"), dir.ops("source-link.jsonl")).unwrap();
    let before = files(&dir);

    for audit in ["symlinked.jsonl", "record-link.jsonl", "source-link.jsonl"] {
        let (restore, _) = dir.restore_auditing(&dir.ops("tenants.json"), &dir.ops(audit), "ops");

        assert_eq!(restore.status.code(), Some(2), "{audit}: {restore:?}");
        assert_eq!(files(&dir), before, "{audit}");
        assert_eq!(std::fs::read(dir.ops("tenants.json")).unwrap(), source, "{audit}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_source_that_is_a_published_file_is_refused_with_no_effect() {
    let dir = torn().await;
    std::os::unix::fs::symlink(dir.published("tenants.json"), dir.ops("alias.json")).unwrap();
    let before = files(&dir);

    for from in [dir.published("tenants.json"), dir.ops("alias.json")] {
        let (restore, _) = dir.restore(&from, "ops");
        assert_eq!(restore.status.code(), Some(2), "{restore:?}");
    }
    assert_eq!(files(&dir), before);
    assert!(!dir.ops("audit.jsonl").exists());
}

#[tokio::test]
async fn a_source_at_a_reserved_staging_path_is_refused_and_kept() {
    // Review 5490404649's probe: `--from .tenants.json.tmp` holding the
    // exact recovery bytes was accepted, and clearing stale staging then
    // deleted the operator's recovery copy.
    let dir = torn().await;
    let staging = dir.published(".tenants.json.tmp");
    std::fs::copy(dir.ops("tenants.json"), &staging).unwrap();
    let before = files(&dir);

    let (restore, _) = dir.restore(&staging, "ops");

    assert_eq!(restore.status.code(), Some(2), "{restore:?}");
    assert_eq!(files(&dir), before);
    assert_eq!(
        std::fs::read(&staging).unwrap(),
        std::fs::read(dir.ops("tenants.json")).unwrap()
    );
    assert!(!dir.ops("audit.jsonl").exists());
}

#[tokio::test]
async fn restore_refuses_an_audit_log_beside_the_published_files() {
    let dir = torn().await;
    let before = files(&dir);

    let (restore, _) = dir.restore_auditing(&dir.ops("tenants.json"), &dir.published("audit.jsonl"), "ops");

    assert_eq!(restore.status.code(), Some(2), "{restore:?}");
    assert_eq!(files(&dir), before);
}

#[tokio::test]
async fn a_held_value_never_reaches_inspect_or_validate_output() {
    // Review 5490101140's probe: a private sentinel in an invalid
    // isolation value came back out through both actions.
    let dir = torn().await;
    let sentinel = "SENTINEL-private-7f3a";
    let mut tenants: Value =
        serde_json::from_slice(&std::fs::read(dir.ops("tenants.json")).unwrap()).unwrap();
    for tenant in tenants.as_array_mut().unwrap() {
        for binding in tenant["data"].as_object_mut().unwrap().values_mut() {
            binding["isolation"] = serde_json::json!({ sentinel: {} });
        }
    }
    std::fs::write(
        dir.published("tenants.json"),
        serde_json::to_vec(&tenants).unwrap(),
    )
    .unwrap();

    for action in ["inspect", "validate"] {
        let (output, _) = dir.run(action, &[]);
        let printed = [output.stdout.as_slice(), output.stderr.as_slice()].concat();
        assert!(
            !String::from_utf8_lossy(&printed).contains(sentinel),
            "{action}: {output:?}"
        );
    }
}

#[tokio::test]
async fn a_kubernetes_restore_is_refused_before_anything_is_read_or_audited() {
    let dir = Dir::new();
    let output = Command::new(env!("CARGO_BIN_EXE_fabric-control-plane-api"))
        .args([
            "held-publication",
            "restore",
            "--namespace",
            "platform-system",
            "--document",
            "tenants",
        ])
        .args(["--from", "/nonexistent/tenants.json", "--audit"])
        .arg(dir.ops("audit.jsonl"))
        .args(["--operator", "ops", "--reason", "x"])
        .env_clear()
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["refused"].as_str().unwrap().contains("ownership"));
    assert!(!dir.ops("audit.jsonl").exists());
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
