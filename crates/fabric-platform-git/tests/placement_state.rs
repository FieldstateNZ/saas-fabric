//! Recording placements means the whole document, in one commit, and
//! nothing else -- the header preserved, a stale precondition refused.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use fabric_core::Clock;
use fabric_git_host::GitCredential;
use fabric_platform_git::{PlatformGitRepository, PlatformRepositoryConfig};
use fabric_platform_management::{DesiredRevision, DesiredStateError, PlacementRecord, PlacementState};

mod support;

use support::{FakePlatformHost, BRANCH, OWNER, REPOSITORY};

const PLACEMENTS: &str = "environments/lucentroot/placements.yaml";

/// A placements document with a hand-written header, standing in for
/// break-glass -- an operator's own words, which this crate must not
/// replace with its own.
const HAND_EDITED: &str = r"# hand edited by an operator under break-glass
---
schemaVersion: 1
environment: lucentroot
placements:
- tenant: existing
  logical: primary
  data_source: shared-postgres-nz-01
  isolation:
    kind: discriminator
    column: tenant_key
    value: existing
  placed_at: 2026-09-18T02:14:00Z
";

fn placement(tenant: &str) -> PlacementRecord {
    let text = format!(
        r"
tenant: {tenant}
logical: primary
data_source: shared-postgres-nz-01
isolation:
  kind: discriminator
  column: tenant_key
  value: {tenant}
placed_at: 2026-09-18T02:14:00Z
"
    );

    serde_norway::from_str(&text).expect("a valid placement fixture")
}

struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_787_907_600
    }
}

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

async fn empty_host() -> FakePlatformHost {
    FakePlatformHost::start(&[]).await
}

#[tokio::test]
async fn an_absent_file_reads_as_an_empty_placement_set_with_no_revision() {
    let host = empty_host().await;
    let repository = repository(&host);

    let read = repository.read_placements("lucentroot").await.unwrap();

    assert_eq!(read.revision, None);
    assert!(read.placements.is_empty());
}

#[tokio::test]
async fn a_first_placement_creates_the_file_with_the_header_in_one_commit() {
    let host = empty_host().await;
    let repository = repository(&host);

    repository
        .write_placements("lucentroot", &[placement("acme")], None, "Place acme primary")
        .await
        .unwrap();

    assert_eq!(host.ref_updates(), 1);

    let written = host.current(PLACEMENTS).unwrap();
    assert!(
        written.starts_with("# Which data source each tenant's intent is placed on"),
        "{written}"
    );
    assert!(written.contains("schemaVersion: 1"));
    assert!(written.contains("tenant: acme"));
}

#[tokio::test]
async fn a_replace_preserves_a_hand_written_header_and_comments() {
    let host = FakePlatformHost::start(&[(PLACEMENTS, HAND_EDITED)]).await;
    let repository = repository(&host);

    let at = repository
        .read_placements("lucentroot")
        .await
        .unwrap()
        .revision
        .unwrap();

    repository
        .write_placements(
            "lucentroot",
            &[placement("acme")],
            Some(&at),
            "Place acme primary",
        )
        .await
        .unwrap();

    let written = host.current(PLACEMENTS).unwrap();
    assert!(
        written.starts_with("# hand edited by an operator under break-glass"),
        "{written}"
    );
    assert!(written.contains("tenant: acme"), "{written}");
    assert!(
        !written.contains("existing"),
        "a replace writes exactly the placements it was given: {written}"
    );
}

#[tokio::test]
async fn a_stale_revision_is_refused_with_conflict() {
    let host = FakePlatformHost::start(&[(PLACEMENTS, HAND_EDITED)]).await;
    let repository = repository(&host);

    let stale = DesiredRevision::new("never-read");

    let failure = repository
        .write_placements(
            "lucentroot",
            &[placement("acme")],
            Some(&stale),
            "Place acme primary",
        )
        .await
        .expect_err("a revision this build never read is stale");

    assert_eq!(
        failure,
        DesiredStateError::Conflict,
        "a stale revision must be refused, not applied"
    );
    assert!(host.current(PLACEMENTS).unwrap().contains("existing"));
}

#[tokio::test]
async fn creating_when_the_file_already_exists_is_refused_with_conflict() {
    let host = FakePlatformHost::start(&[(PLACEMENTS, HAND_EDITED)]).await;
    let repository = repository(&host);

    let failure = repository
        .write_placements("lucentroot", &[placement("acme")], None, "Place acme primary")
        .await
        .expect_err("None means create, and a file is already there");

    assert_eq!(failure, DesiredStateError::Conflict);
}

#[tokio::test]
async fn a_document_describing_another_environment_is_refused() {
    let wrong_environment: &str = r"---
schemaVersion: 1
environment: other
placements: []
";
    let host = FakePlatformHost::start(&[(PLACEMENTS, wrong_environment)]).await;
    let repository = repository(&host);

    let failure = repository
        .read_placements("lucentroot")
        .await
        .expect_err("the document describes a different environment");

    assert!(
        matches!(failure, DesiredStateError::Refused { .. }),
        "{failure:?}"
    );
}

#[tokio::test]
async fn a_document_declaring_the_wrong_schema_version_is_refused() {
    let wrong_schema: &str = r"---
schemaVersion: 2
environment: lucentroot
placements: []
";
    let host = FakePlatformHost::start(&[(PLACEMENTS, wrong_schema)]).await;
    let repository = repository(&host);

    let failure = repository
        .read_placements("lucentroot")
        .await
        .expect_err("schemaVersion 2 is not a version this build reads");

    assert!(
        matches!(failure, DesiredStateError::Refused { .. }),
        "{failure:?}"
    );
}
