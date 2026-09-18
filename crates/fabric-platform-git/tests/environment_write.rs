//! `write_environment` writes both documents in one commit, ADR 0023 part 2
//! (B4) -- the adapter's half of the fix for `place`/`remove` interleaving.

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
use fabric_platform_management::{
    DataSourceState, DesiredRevision, DesiredStateError, EnvironmentWrite, PlacementState, PlatformRepository,
};

mod support;

use support::{FakePlatformHost, BRANCH, OWNER, REPOSITORY};

const DATA_SOURCES: &str = "environments/lucentroot/data-sources.yaml";
const PLACEMENTS: &str = "environments/lucentroot/placements.yaml";

const HAND_EDITED_DATA_SOURCES: &str = r"# hand edited by an operator under break-glass
---
schemaVersion: 1
environment: lucentroot
dataSources:
- id: shared-a
  revision: 1
  connector: postgres-nz
  connection:
    kind: named
    name: shared
  placement: dedicated
  residency:
    region: nz
  pool:
    max_connections: 20
    idle_timeout_seconds: 300
    acquire_timeout_seconds: 5
  capabilities:
    writable: true
    accepts_new_tenants: true
  labels: {}
";

const HAND_EDITED_PLACEMENTS: &str = r"# hand edited by an operator under break-glass
---
schemaVersion: 1
environment: lucentroot
placements:
- tenant: existing
  logical: primary
  data_source: shared-a
  isolation:
    kind: discriminator
    column: tenant_key
    value: existing
  placed_at: 2026-09-18T02:14:00Z
";

fn declaration(id: &str) -> fabric_platform_management::DataSourceDeclaration {
    let text = format!(
        r"
id: {id}
revision: 0
connector: postgres-nz
connection:
  kind: named
  name: shared
placement: dedicated
residency:
  region: nz
"
    );
    serde_norway::from_str(&text).expect("a valid declaration fixture")
}

fn placement(tenant: &str) -> fabric_platform_management::PlacementRecord {
    let text = format!(
        r"
tenant: {tenant}
logical: primary
data_source: shared-a
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

#[tokio::test]
async fn creating_both_documents_for_the_first_time_is_one_commit() {
    let host = FakePlatformHost::start(&[]).await;
    let repository = repository(&host);

    repository
        .write_environment(
            "lucentroot",
            EnvironmentWrite {
                data_sources: (&[declaration("a")], None),
                placements: (&[], None),
            },
            "Declare a",
        )
        .await
        .unwrap();

    assert_eq!(host.ref_updates(), 1, "one commit, both files");
    assert!(host.current(DATA_SOURCES).unwrap().contains("id: a"));
    assert!(
        host.current(PLACEMENTS)
            .unwrap()
            .starts_with("# Which data source each tenant's intent is placed on"),
        "the placements file is still written, empty, with its own header"
    );
}

#[tokio::test]
async fn a_replace_writes_both_documents_at_their_own_revisions_in_one_commit() {
    let host = FakePlatformHost::start(&[
        (DATA_SOURCES, HAND_EDITED_DATA_SOURCES),
        (PLACEMENTS, HAND_EDITED_PLACEMENTS),
    ])
    .await;
    let repository = repository(&host);

    let data_sources_at = repository
        .read_data_sources("lucentroot")
        .await
        .unwrap()
        .revision
        .unwrap();
    let placements_at = repository
        .read_placements("lucentroot")
        .await
        .unwrap()
        .revision
        .unwrap();

    repository
        .write_environment(
            "lucentroot",
            EnvironmentWrite {
                data_sources: (&[declaration("a")], Some(&data_sources_at)),
                placements: (&[placement("acme")], Some(&placements_at)),
            },
            "Place acme primary",
        )
        .await
        .unwrap();

    assert_eq!(host.ref_updates(), 1, "one commit, both files");

    let data_sources = host.current(DATA_SOURCES).unwrap();
    assert!(data_sources.starts_with("# hand edited by an operator under break-glass"));
    assert!(data_sources.contains("id: a"));
    assert!(
        !data_sources.contains("shared-a"),
        "a replace writes exactly the list it was given"
    );

    let placements = host.current(PLACEMENTS).unwrap();
    assert!(placements.starts_with("# hand edited by an operator under break-glass"));
    assert!(placements.contains("tenant: acme"));
    assert!(
        !placements.contains("existing"),
        "a replace writes exactly the list it was given"
    );
}

#[tokio::test]
async fn a_moved_sibling_is_refused_and_neither_document_is_written() {
    let host = FakePlatformHost::start(&[
        (DATA_SOURCES, HAND_EDITED_DATA_SOURCES),
        (PLACEMENTS, HAND_EDITED_PLACEMENTS),
    ])
    .await;
    let repository = repository(&host);

    let data_sources_at = repository
        .read_data_sources("lucentroot")
        .await
        .unwrap()
        .revision
        .unwrap();
    let stale_placements_at = DesiredRevision::new("never-read");

    let failure = repository
        .write_environment(
            "lucentroot",
            EnvironmentWrite {
                data_sources: (&[declaration("a")], Some(&data_sources_at)),
                placements: (&[placement("acme")], Some(&stale_placements_at)),
            },
            "Place acme primary",
        )
        .await
        .expect_err("the placements revision was never read");

    assert_eq!(failure, DesiredStateError::Conflict);
    assert_eq!(host.ref_updates(), 0, "neither document is written");
    assert!(
        host.current(DATA_SOURCES).unwrap().contains("shared-a"),
        "the data sources file is untouched"
    );
    assert!(
        host.current(PLACEMENTS).unwrap().contains("existing"),
        "the placements file is untouched"
    );
}
