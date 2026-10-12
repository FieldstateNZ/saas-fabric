//! Gap G5: a reader's last-known-good generation survives a restart.
//!
//! Every test drives the real `FilesystemRuntimePublication` and the
//! production reader, `build_published_runtime` over `PublishedFiles` with a
//! last-known-good directory, beside the published files rather than inside
//! them. A torn volume is a document overwritten with bytes that do not
//! parse; a mismatched one is a document put back to the previous
//! generation's bytes while the record names the new one, as a Kubernetes
//! volume the kubelet has not yet projected would be. Each generation is
//! told apart by its tenant revision.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fabric_core::{BindingRevision, TenantId};
use fabric_data_api::ResourceCatalog;
use fabric_runtime_publication::RuntimePublication as _;
use fabric_tenant_runtime::{
    build_published_runtime, GenerationOrigin, LastKnownGoodState, LastKnownGoodStore, PublishedFiles,
    PublishedRuntime, ResolveError, Serving, SourceError,
};

/// The replica's own writable directory: a separate temporary directory,
/// never inside the published one, which the reader refuses.
fn last_known_good(lkg: &support::TempDir) -> PathBuf {
    lkg.path().join("copy")
}

fn reader(dir: &support::TempDir, lkg: &support::TempDir) -> PublishedFiles {
    support::published_files(dir).with_last_known_good(LastKnownGoodStore::new(last_known_good(lkg)))
}

async fn start(dir: &support::TempDir, lkg: &support::TempDir) -> Result<PublishedRuntime, String> {
    build_published_runtime(&support::runtime_config(), Arc::new(reader(dir, lkg))).await
}

/// Starts a reader over what is published, so it persists that generation,
/// and stops it: the replica that later restarts.
async fn run_once(dir: &support::TempDir, lkg: &support::TempDir) {
    let runtime = start(dir, lkg).await.unwrap();
    runtime.handles.shutdown().await.unwrap();
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

/// Replaces `document` with `bytes` and names them in the record, so every
/// digest matches: what a faulty publisher, not a torn volume, would leave.
fn publish_matching(dir: &support::TempDir, file: &str, key: &str, bytes: &[u8]) {
    dir.write_raw(file, bytes);
    let mut record: serde_json::Value = serde_json::from_slice(&dir.read_raw("publication.json")).unwrap();
    record["documents"][key]["sha256"] = serde_json::Value::from(sha256_hex(bytes));
    dir.write_raw("publication.json", &serde_json::to_vec_pretty(&record).unwrap());
}

async fn publish(dir: &support::TempDir, revision: u64) {
    dir.publisher()
        .publish(&support::base_snapshot(revision))
        .await
        .unwrap();
}

fn acme_revision(runtime: &PublishedRuntime) -> Result<BindingRevision, ResolveError> {
    runtime
        .resolver
        .resolve_tenant(&TenantId::try_new("acme").unwrap())
        .map(|binding| binding.revision)
}

/// The resource names of the catalogue in the view `runtime` serves.
fn catalogue_names(runtime: &PublishedRuntime) -> Vec<String> {
    runtime
        .resolver
        .pin()
        .catalogue()
        .and_then(|catalogue| catalogue.get::<ResourceCatalog>())
        .map(|catalogue| catalogue.names().map(ToString::to_string).collect())
        .unwrap_or_default()
}

fn serving(runtime: &PublishedRuntime) -> Serving {
    runtime.status.report().serving
}

async fn refresh_until(
    runtime: &PublishedRuntime,
    what: &str,
    condition: impl Fn(&PublishedRuntime) -> bool,
) {
    let start = Instant::now();
    runtime.handles.refresh_now();
    while !condition(runtime) {
        assert!(start.elapsed() < Duration::from_secs(2), "{what} did not happen");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The copy is a complete triple (G8): a restart that serves it serves its
/// catalogue, never a newer one mounted beside a torn tenants file.
#[tokio::test]
async fn a_restart_serving_its_copy_serves_the_copys_catalogue_not_a_newer_mounted_one() {
    let dir = support::TempDir::new();
    let lkg = support::TempDir::new();
    publish(&dir, 1).await;
    run_once(&dir, &lkg).await;
    let mut second = support::base_snapshot(2);
    let mut resources: std::collections::BTreeMap<_, _> = support::articles_catalog()
        .resources()
        .map(|(name, definition)| (name.clone(), definition.clone()))
        .collect();
    let articles = resources.values().next().unwrap().clone();
    resources.insert(
        fabric_core::LogicalResourceName::try_new("posts").unwrap(),
        articles,
    );
    second.catalog = fabric_runtime_publication::DocumentInput::new(
        fabric_runtime_publication::DocumentRevision::new(2),
        fabric_runtime_publication::CatalogDocument::new(resources),
    );
    dir.publisher().publish(&second).await.unwrap();

    dir.write_raw("tenants.json", b"{ torn");
    let restarted = start(&dir, &lkg).await.unwrap();

    assert_eq!(acme_revision(&restarted).unwrap(), BindingRevision::new(1));
    assert_eq!(catalogue_names(&restarted), ["articles"]);
    assert_eq!(
        restarted
            .resolver
            .served_generation()
            .map(|report| report.generation),
        Some(1)
    );
    restarted.handles.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_replica_restarting_on_a_torn_volume_serves_its_last_known_good_generation() {
    let dir = support::TempDir::new();
    let lkg = support::TempDir::new();
    publish(&dir, 1).await;
    run_once(&dir, &lkg).await;

    dir.write_raw("data-sources.json", b"{ torn");
    let restarted = start(&dir, &lkg).await.unwrap();

    assert_eq!(acme_revision(&restarted).unwrap(), BindingRevision::new(1));
    assert_eq!(
        catalogue_names(&restarted),
        ["articles"],
        "the copy's own catalogue"
    );
    let report = restarted.status.report();
    assert!(report.holding_last_known_good(), "{report:?}");
    let Serving::Generation {
        generation: 1,
        origin: GenerationOrigin::LastKnownGood { held_because },
    } = report.serving
    else {
        panic!("expected generation 1 from last-known-good, got {report:?}");
    };
    assert!(held_because.contains("data-sources"), "{held_because}");
    assert_eq!(
        report.last_known_good,
        LastKnownGoodState::Stored { generation: 1 }
    );
    restarted.handles.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_restart_on_a_torn_volume_with_no_last_known_good_fails_closed() {
    let dir = support::TempDir::new();
    let lkg = support::TempDir::new();
    publish(&dir, 1).await;
    dir.write_raw("data-sources.json", b"{ torn");

    let refused = start(&dir, &lkg).await;

    assert!(
        refused.is_err(),
        "nothing valid to prime from must fail the prime"
    );
    assert!(!LastKnownGoodStore::new(last_known_good(&lkg)).path().exists());

    let unprimed = build_published_runtime(
        &fabric_tenant_runtime::RuntimeConfig {
            fail_fast_on_prime: false,
            ..support::runtime_config()
        },
        Arc::new(reader(&dir, &lkg)),
    )
    .await
    .unwrap();
    assert!(matches!(
        acme_revision(&unprimed),
        Err(ResolveError::RuntimeUnavailable)
    ));
    assert_eq!(serving(&unprimed), Serving::Nothing);
    assert_eq!(
        unprimed.status.report().last_known_good,
        LastKnownGoodState::Absent
    );
    unprimed.handles.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_corrupted_last_known_good_is_refused_left_in_place_and_the_restart_fails_closed() {
    let dir = support::TempDir::new();
    let lkg = support::TempDir::new();
    publish(&dir, 1).await;
    run_once(&dir, &lkg).await;
    let copy = LastKnownGoodStore::new(last_known_good(&lkg))
        .path()
        .to_path_buf();
    let corrupted = String::from_utf8(std::fs::read(&copy).unwrap())
        .unwrap()
        .replacen("acme", "acmf", 1);
    std::fs::write(&copy, &corrupted).unwrap();

    dir.write_raw("data-sources.json", b"{ torn");
    let refused = start(&dir, &lkg).await;

    assert!(refused.is_err());
    assert_eq!(
        std::fs::read_to_string(&copy).unwrap(),
        corrupted,
        "a refused copy is never rewritten or removed"
    );
}

#[tokio::test]
async fn a_stale_last_known_good_loses_to_a_newer_published_generation_and_is_replaced() {
    let dir = support::TempDir::new();
    let lkg = support::TempDir::new();
    publish(&dir, 1).await;
    run_once(&dir, &lkg).await;
    publish(&dir, 2).await;

    let restarted = start(&dir, &lkg).await.unwrap();

    assert_eq!(acme_revision(&restarted).unwrap(), BindingRevision::new(2));
    assert_eq!(
        serving(&restarted),
        Serving::Generation {
            generation: 2,
            origin: GenerationOrigin::Published,
        }
    );
    assert_eq!(
        restarted.status.report().last_known_good,
        LastKnownGoodState::Stored { generation: 2 }
    );
    restarted.handles.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_replica_holding_last_known_good_rolls_forward_the_moment_a_newer_generation_validates() {
    let dir = support::TempDir::new();
    let lkg = support::TempDir::new();
    publish(&dir, 1).await;
    run_once(&dir, &lkg).await;
    let first = dir.capture(&["data-sources.json"]);
    publish(&dir, 2).await;
    let second = dir.capture(&["data-sources.json"]);

    dir.restore(&first);
    let restarted = start(&dir, &lkg).await.unwrap();
    assert_eq!(acme_revision(&restarted).unwrap(), BindingRevision::new(1));
    assert!(restarted.status.report().holding_last_known_good());

    dir.restore(&second);
    refresh_until(&restarted, "rolling forward to generation 2", |runtime| {
        acme_revision(runtime) == Ok(BindingRevision::new(2))
    })
    .await;

    assert!(!restarted.status.report().holding_last_known_good());
    assert_eq!(
        restarted.status.report().last_known_good,
        LastKnownGoodState::Stored { generation: 2 }
    );
    restarted.handles.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_running_replica_holds_against_a_complete_generation_below_the_one_it_holds() {
    let dir = support::TempDir::new();
    let lkg = support::TempDir::new();
    publish(&dir, 1).await;
    let first = dir.capture(&[
        "tenants.json",
        "data-sources.json",
        "catalog.json",
        "publication.json",
    ]);
    publish(&dir, 2).await;
    let runtime = start(&dir, &lkg).await.unwrap();
    let held_before = runtime.status.report().held_passes;

    dir.restore(&first);
    refresh_until(&runtime, "a held pass", |runtime| {
        runtime.status.report().held_passes > held_before
    })
    .await;

    assert_eq!(acme_revision(&runtime).unwrap(), BindingRevision::new(2));
    let last_hold = runtime.status.report().last_hold.unwrap();
    let expected = SourceError::Regressed {
        origin: String::new(),
        held: 2,
        found: "generation 1".to_owned(),
    }
    .to_string();
    assert!(last_hold.ends_with(expected.trim_start()), "{last_hold}");
    runtime.handles.shutdown().await.unwrap();
}

#[tokio::test]
async fn without_a_last_known_good_directory_the_reader_writes_nothing_and_behaves_as_before() {
    let dir = support::TempDir::new();
    let lkg = support::TempDir::new();
    publish(&dir, 1).await;
    let files = support::published_files(&dir);

    let runtime = build_published_runtime(&support::runtime_config(), Arc::new(files))
        .await
        .unwrap();

    assert_eq!(
        runtime.status.report().last_known_good,
        LastKnownGoodState::Disabled
    );
    assert!(!last_known_good(&lkg).exists());
    runtime.handles.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_higher_generation_whose_tenants_match_their_digest_but_do_not_decode_never_displaces_the_copy() {
    let dir = support::TempDir::new();
    let lkg = support::TempDir::new();
    publish(&dir, 1).await;
    run_once(&dir, &lkg).await;
    let copy = LastKnownGoodStore::new(last_known_good(&lkg))
        .path()
        .to_path_buf();
    let before = std::fs::read(&copy).unwrap();
    publish(&dir, 2).await;
    publish_matching(&dir, "tenants.json", "tenants", b"[{ \"tenant\": ");

    let restarted = start(&dir, &lkg).await.unwrap();
    assert_eq!(acme_revision(&restarted).unwrap(), BindingRevision::new(1));
    assert!(restarted.status.report().holding_last_known_good());
    assert_eq!(
        std::fs::read(&copy).unwrap(),
        before,
        "the copy survives the failure it exists for"
    );
    restarted.handles.shutdown().await.unwrap();

    let again = start(&dir, &lkg).await.unwrap();
    assert_eq!(
        acme_revision(&again).unwrap(),
        BindingRevision::new(1),
        "and survives the next restart"
    );
    again.handles.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_running_replica_holds_against_a_higher_generation_whose_catalogue_the_host_cannot_decode() {
    let dir = support::TempDir::new();
    let lkg = support::TempDir::new();
    publish(&dir, 1).await;
    let runtime = start(&dir, &lkg).await.unwrap();
    let copy = LastKnownGoodStore::new(last_known_good(&lkg))
        .path()
        .to_path_buf();
    let before = std::fs::read(&copy).unwrap();
    let held_before = runtime.status.report().held_passes;

    publish(&dir, 2).await;
    publish_matching(&dir, "catalog.json", "catalog", b"[\"not a catalogue\"]");
    refresh_until(&runtime, "a held pass", |runtime| {
        runtime.status.report().held_passes > held_before
    })
    .await;

    assert_eq!(acme_revision(&runtime).unwrap(), BindingRevision::new(1));
    assert!(runtime.status.report().last_hold.unwrap().contains("catalog"));
    assert_eq!(std::fs::read(&copy).unwrap(), before);
    runtime.handles.shutdown().await.unwrap();
}
