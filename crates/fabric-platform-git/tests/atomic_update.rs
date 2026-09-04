//! One desired-state change is one commit, and a race is settled by which
//! files moved rather than by whether the branch did.
//!
//! Every test here drives the adapter against a Git host answering over a real
//! socket. The fake applies a commit only when its parent is the current head,
//! which is the fast-forward rule the whole design rests on — so a test that
//! passes here is a test of the mechanism and not of a mock's opinion of it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use fabric_core::Clock;
use fabric_git_host::GitCredential;
use fabric_platform_git::{
    CommitRevision, FileChange, FileRevision, PlatformGitError, PlatformGitRepository,
    PlatformRepositoryConfig,
};

mod support;

use support::{FakePlatformHost, BRANCH, OWNER, REPOSITORY};

/// The three files one SaaS Fabric promotion touches.
const RECORD: &str = "environments/lucentroot/promotions/saas-fabric.yaml";
const RUNTIME: &str = "applications/core/saas-fabric/overlays/lucentroot/kustomization.yaml";
const CONSOLE: &str = "applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml";

/// A clock that does not move; nothing here measures time.
struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_787_907_600
    }
}

/// The repository, pointed at a fake.
fn repository(host: &FakePlatformHost) -> PlatformGitRepository {
    PlatformGitRepository::new(
        &PlatformRepositoryConfig {
            api_base_url: host.base_url.clone(),
            owner: OWNER.to_owned(),
            repository: REPOSITORY.to_owned(),
            branch: BRANCH.to_owned(),
            http_timeout_seconds: 5,
            operation_timeout_seconds: 30,
        },
        GitCredential::token("test-bearer"),
        Arc::new(TestClock),
    )
    .unwrap()
}

/// A host holding the three files, with recognisable starting content.
async fn host_with_the_three_files() -> FakePlatformHost {
    FakePlatformHost::start(&[
        (RECORD, "version: 0.2.2\n"),
        (RUNTIME, "newTag: 0.2.2\n"),
        (CONSOLE, "newTag: 0.2.2\n"),
    ])
    .await
}

/// The change that moves all three onto a preview, at the revisions read.
async fn move_to_preview(
    repository: &PlatformGitRepository,
    at: &CommitRevision,
) -> Result<Vec<FileChange>, PlatformGitError> {
    let mut changes = Vec::new();

    for (path, text) in [
        (RECORD, "version: 0.3.0-preview.1\n"),
        (RUNTIME, "newTag: 0.3.0-preview.1\n"),
        (CONSOLE, "newTag: 0.3.0-preview.1\n"),
    ] {
        let stored = repository.read(path, at).await?;
        changes.push(FileChange {
            path: path.to_owned(),
            text: text.to_owned(),
            expected: Some(stored.revision),
        });
    }

    Ok(changes)
}

#[tokio::test]
async fn an_uncontested_change_is_one_commit_carrying_every_file() {
    let host = host_with_the_three_files().await;
    let repository = repository(&host);

    let base = repository.head().await.unwrap();
    let changes = move_to_preview(&repository, &base).await.unwrap();

    let commit = repository
        .update_files_atomically(&base, &changes, "Promote LucentRoot")
        .await
        .unwrap();

    assert_eq!(
        host.ref_updates(),
        1,
        "an uncontested change should move the branch once"
    );
    assert_ne!(commit, base);

    // The property that matters: all three arrived, and they arrived together.
    for path in [RECORD, RUNTIME, CONSOLE] {
        assert_eq!(
            host.current(path).unwrap().trim(),
            match path {
                RECORD => "version: 0.3.0-preview.1",
                _ => "newTag: 0.3.0-preview.1",
            },
            "{path} did not land"
        );
    }
}

#[tokio::test]
async fn an_unrelated_commit_costs_a_retry_and_nothing_else() {
    let host = host_with_the_three_files().await;
    let repository = repository(&host);

    let base = repository.head().await.unwrap();
    let changes = move_to_preview(&repository, &base).await.unwrap();

    // Somebody merges an unrelated pull request in the window between the head
    // being read and the branch being asked to move.
    host.someone_else_commits(&[("applications/core/keycloak/README.md", "unrelated\n")]);

    repository
        .update_files_atomically(&base, &changes, "Promote LucentRoot")
        .await
        .expect("an unrelated commit must not become an operator's problem");

    assert_eq!(
        host.ref_updates(),
        2,
        "the first attempt should have been refused"
    );
    assert_eq!(host.current(RECORD).unwrap().trim(), "version: 0.3.0-preview.1");
    assert_eq!(
        host.current("applications/core/keycloak/README.md")
            .unwrap()
            .trim(),
        "unrelated",
        "the retry must rebuild on the other commit rather than discard it"
    );
}

#[tokio::test]
async fn a_change_to_a_file_being_written_is_refused_rather_than_overwritten() {
    let host = host_with_the_three_files().await;
    let repository = repository(&host);

    let base = repository.head().await.unwrap();
    let changes = move_to_preview(&repository, &base).await.unwrap();

    // This time the other commit touches a file this write is editing.
    host.someone_else_commits(&[(RUNTIME, "newTag: 0.2.3\n")]);

    let failure = repository
        .update_files_atomically(&base, &changes, "Promote LucentRoot")
        .await
        .expect_err("a relevant change must be a conflict");

    assert_eq!(
        failure,
        PlatformGitError::Conflict {
            path: RUNTIME.to_owned()
        }
    );

    // Nothing of ours was applied, including the two files nobody touched.
    assert_eq!(host.current(RUNTIME).unwrap().trim(), "newTag: 0.2.3");
    assert_eq!(host.current(RECORD).unwrap().trim(), "version: 0.2.2");
    assert_eq!(host.current(CONSOLE).unwrap().trim(), "newTag: 0.2.2");
}

#[tokio::test]
async fn a_branch_that_never_settles_gives_up_rather_than_looping() {
    let host = host_with_the_three_files().await;
    let repository = repository(&host);

    let base = repository.head().await.unwrap();
    let changes = move_to_preview(&repository, &base).await.unwrap();

    // More unrelated commits than the adapter will ever retry.
    for index in 0..10 {
        host.someone_else_commits(&[("docs/notes.md", Box::leak(format!("{index}\n").into_boxed_str()))]);
    }

    let failure = repository
        .update_files_atomically(&base, &changes, "Promote LucentRoot")
        .await
        .expect_err("a branch that never settles must not retry forever");

    assert_eq!(failure, PlatformGitError::Contended);
    assert!(
        host.ref_updates() <= 8,
        "retries must be bounded, saw {}",
        host.ref_updates()
    );
    assert_eq!(
        host.current(RECORD).unwrap().trim(),
        "version: 0.2.2",
        "giving up must leave desired state alone"
    );
}

#[tokio::test]
async fn only_a_409_is_read_as_contention() {
    for (status, expected) in [
        (
            500_u16,
            PlatformGitError::Unavailable {
                detail: "moving the branch returned 500".to_owned(),
            },
        ),
        (
            422,
            PlatformGitError::Rejected {
                detail: "moving the branch was refused with 422".to_owned(),
            },
        ),
        (403, PlatformGitError::NotPermitted),
    ] {
        let host = host_with_the_three_files().await;
        let repository = repository(&host);

        let base = repository.head().await.unwrap();
        let changes = move_to_preview(&repository, &base).await.unwrap();
        host.ref_update_answers(status);

        let failure = repository
            .update_files_atomically(&base, &changes, "Promote LucentRoot")
            .await
            .expect_err("a non-409 status is a failure, not a race");

        assert_eq!(failure, expected, "status {status}");
        assert_eq!(
            host.ref_updates(),
            1,
            "status {status} must not be retried as though it were contention"
        );
    }
}

#[tokio::test]
async fn the_branch_is_never_forced() {
    let host = host_with_the_three_files().await;
    let repository = repository(&host);

    let base = repository.head().await.unwrap();
    let changes = move_to_preview(&repository, &base).await.unwrap();
    host.someone_else_commits(&[("docs/notes.md", "unrelated\n")]);

    repository
        .update_files_atomically(&base, &changes, "Promote LucentRoot")
        .await
        .unwrap();

    assert!(!host.was_forced(), "the adapter must never send force: true");
}

#[tokio::test]
async fn no_source_file_can_ask_for_a_force() {
    // The runtime assertion above only covers the paths a test exercises. This
    // covers the ones nobody has written yet: a `force: true` anywhere in this
    // crate is a way to lose a commit nobody knew about, and there is no
    // circumstance in which this adapter wants one.
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();

    for entry in walk(&source) {
        let text = std::fs::read_to_string(&entry).unwrap();

        // Comment lines are skipped, because the crate documentation says the
        // words "force: true" in the course of explaining that no code sends
        // them. A scan that could not tell prose from code would either fail
        // here forever or be deleted, and both are worse than reading code.
        let asks = text
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with("//"))
            .any(|line| line.contains("force: true") || line.contains("\"force\":true"));

        if asks {
            offenders.push(entry.display().to_string());
        }
    }

    assert!(offenders.is_empty(), "force is asked for in {offenders:?}");
}

#[tokio::test]
async fn a_change_with_no_files_is_refused() {
    let host = host_with_the_three_files().await;
    let repository = repository(&host);
    let base = repository.head().await.unwrap();

    let failure = repository
        .update_files_atomically(&base, &[], "Nothing")
        .await
        .expect_err("an empty change must not mint a commit");

    assert!(matches!(failure, PlatformGitError::Rejected { .. }));
    assert_eq!(host.ref_updates(), 0);
}

#[tokio::test]
async fn a_file_that_appeared_since_the_read_is_a_conflict() {
    let host = FakePlatformHost::start(&[(RUNTIME, "newTag: 0.2.2\n")]).await;
    let repository = repository(&host);

    let base = repository.head().await.unwrap();
    let stored = repository.read(RUNTIME, &base).await.unwrap();

    // The record does not exist yet, so this write expects to create it.
    let changes = vec![
        FileChange {
            path: RECORD.to_owned(),
            text: "version: 0.3.0-preview.1\n".to_owned(),
            expected: None,
        },
        FileChange {
            path: RUNTIME.to_owned(),
            text: "newTag: 0.3.0-preview.1\n".to_owned(),
            expected: Some(stored.revision),
        },
    ];

    // Somebody creates it first.
    host.someone_else_commits(&[(RECORD, "version: 0.2.9\n")]);

    let failure = repository
        .update_files_atomically(&base, &changes, "Promote LucentRoot")
        .await
        .expect_err("a file appearing where none was expected is a change");

    assert_eq!(
        failure,
        PlatformGitError::Conflict {
            path: RECORD.to_owned()
        }
    );
    assert_eq!(host.current(RECORD).unwrap().trim(), "version: 0.2.9");
}

#[tokio::test]
async fn a_revision_read_is_the_revision_sent_back() {
    // Guards the seam between reading and writing: if `read` reported a
    // revision the write did not carry, every conflict check above would be
    // comparing something to itself and would pass regardless.
    let host = host_with_the_three_files().await;
    let repository = repository(&host);

    let base = repository.head().await.unwrap();
    let stored = repository.read(RECORD, &base).await.unwrap();

    assert_eq!(stored.text, "version: 0.2.2\n");
    assert_ne!(stored.revision, FileRevision::new("not-the-hash"));

    host.someone_else_commits(&[(RECORD, "version: 0.2.9\n")]);

    let failure = repository
        .update_files_atomically(
            &base,
            &[FileChange {
                path: RECORD.to_owned(),
                text: "version: 0.3.0-preview.1\n".to_owned(),
                expected: Some(stored.revision),
            }],
            "Promote LucentRoot",
        )
        .await
        .expect_err("the revision read must be the one compared against");

    assert_eq!(
        failure,
        PlatformGitError::Conflict {
            path: RECORD.to_owned()
        }
    );
}

/// Every `.rs` file under a directory.
fn walk(directory: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();

    for entry in std::fs::read_dir(directory).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }

    found
}
