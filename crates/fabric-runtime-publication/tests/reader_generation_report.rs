//! What a replica reports serving is the record the publisher stamped (G7).
//!
//! The real filesystem publisher writes; the real published reader primes and
//! refreshes from the same files. The control plane compares a replica's
//! report with the publisher's `RecordStamp`, so the two must agree byte for
//! byte: the same generation, and the SHA-256 of the same record bytes.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fabric_core::{ReaderGenerationReport, ServedOrigin};
use fabric_runtime_publication::{
    DocumentOutcome, RecordStamp, RuntimePublication as _, PUBLICATION_RECORD_FILE,
};
use fabric_tenant_runtime::{
    build_published_runtime, PublishedRuntime, PublishedStateSource, RuntimeConfig, RuntimeResolver,
};
use support::{base_snapshot, TempDir};

fn reported(stamp: RecordStamp) -> ReaderGenerationReport {
    ReaderGenerationReport::new(stamp.generation, stamp.sha256, ServedOrigin::Published)
}

async fn reader(dir: &TempDir) -> PublishedRuntime {
    let files = support::published_files(dir);
    let config = RuntimeConfig {
        refresh_interval_seconds: 60,
        fail_fast_on_prime: true,
        ..RuntimeConfig::default()
    };
    build_published_runtime(&config, Arc::new(files) as Arc<dyn PublishedStateSource>)
        .await
        .unwrap()
}

async fn until_served(resolver: &RuntimeResolver, wanted: ReaderGenerationReport) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while resolver.served_generation() != Some(wanted) {
        assert!(Instant::now() < deadline, "the replica never reported {wanted:?}");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

#[tokio::test]
async fn a_replica_reports_the_record_the_publisher_stamped_at_start_and_after_a_refresh() {
    let dir = TempDir::new();
    let first = dir.publisher().publish(&base_snapshot(1)).await.unwrap().record;
    assert_eq!(first.generation, 1);
    let held = dir.read_raw(PUBLICATION_RECORD_FILE);
    assert_eq!(
        first.sha256.to_string(),
        format!("{:x}", Sha(&held)),
        "the stamp is the SHA-256 of the bytes on disk"
    );

    let runtime = reader(&dir).await;
    assert_eq!(runtime.resolver.served_generation(), Some(reported(first)));

    let second = dir.publisher().publish(&base_snapshot(2)).await.unwrap().record;
    assert_eq!(second.generation, 2);
    assert_ne!(second.sha256, first.sha256);
    runtime.handles.refresh_now();
    until_served(&runtime.resolver, reported(second)).await;

    runtime.handles.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_unchanged_publication_stamps_the_record_already_held() {
    let dir = TempDir::new();
    let first = dir.publisher().publish(&base_snapshot(1)).await.unwrap();
    let again = dir.publisher().publish(&base_snapshot(1)).await.unwrap();
    assert_eq!(again.tenants, DocumentOutcome::Unchanged);
    assert_eq!(again.record, first.record);

    let runtime = reader(&dir).await;
    assert_eq!(runtime.resolver.served_generation(), Some(reported(again.record)));
    runtime.handles.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_record_rewritten_at_the_same_generation_is_another_generation_to_a_reader() {
    let dir = TempDir::new();
    let stamp = dir.publisher().publish(&base_snapshot(1)).await.unwrap().record;
    let runtime = reader(&dir).await;

    let held = String::from_utf8(dir.read_raw(PUBLICATION_RECORD_FILE)).unwrap();
    let respaced = format!("{held}\n");
    dir.write_raw(PUBLICATION_RECORD_FILE, respaced.as_bytes());
    runtime.handles.refresh_now();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let served = runtime.resolver.served_generation().unwrap();
        if served != reported(stamp) {
            assert_eq!(served.generation, stamp.generation);
            assert_ne!(served.record_sha256, stamp.sha256);
            break;
        }
        assert!(Instant::now() < deadline, "the respaced record was never read");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    runtime.handles.shutdown().await.unwrap();
}

/// A lowercase-hex SHA-256, computed independently of both crates.
struct Sha<'a>(&'a [u8]);

impl std::fmt::LowerHex for Sha<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ring::digest::digest(&ring::digest::SHA256, self.0)
            .as_ref()
            .iter()
            .try_for_each(|byte| write!(formatter, "{byte:02x}"))
    }
}
