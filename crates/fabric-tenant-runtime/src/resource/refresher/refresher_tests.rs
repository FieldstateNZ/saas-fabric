//! Priming, triggered refresh, and surviving a failed load.

use std::sync::Arc;
use std::time::Duration;

use fabric_core::BindingRevision;

use crate::resource::sources::InMemorySource;
use crate::resource::{ResourceRefresher, ResourceSource};
use crate::testing::{tenant, tenant_binding};
use crate::{RuntimeConfig, TenantRegistry, TenantRuntimeBinding};

/// A long interval, so these tests exercise the trigger rather than the poll.
fn config() -> RuntimeConfig {
    RuntimeConfig {
        refresh_interval_seconds: 3600,
        fail_fast_on_prime: true,
    }
}

#[tokio::test]
async fn priming_makes_the_registry_servable() {
    let registry = TenantRegistry::new();
    let source = InMemorySource::new(vec![tenant_binding("acme", 1, "shared-01")]);

    let count = ResourceRefresher::prime(&registry, &source).await.unwrap();

    assert_eq!(count, 1);
    assert!(registry.is_primed());
    assert!(registry.lookup(&tenant("acme")).is_ok());
}

#[tokio::test]
async fn priming_from_a_failing_source_leaves_the_registry_unprimed() {
    let registry = TenantRegistry::new();
    let source: InMemorySource<TenantRuntimeBinding> = InMemorySource::empty();
    source.fail_next();

    assert!(ResourceRefresher::prime(&registry, &source).await.is_err());
    assert!(!registry.is_primed());
}

#[tokio::test]
async fn a_triggered_refresh_picks_up_a_new_revision() {
    let registry = Arc::new(TenantRegistry::new());
    let source = Arc::new(InMemorySource::new(vec![tenant_binding("acme", 1, "shared-01")]));

    ResourceRefresher::prime(&registry, source.as_ref())
        .await
        .unwrap();

    let mut changes = registry.subscribe();
    let handle = ResourceRefresher::spawn(
        Arc::clone(&registry),
        Arc::clone(&source) as Arc<dyn ResourceSource<TenantRuntimeBinding>>,
        &config(),
    );

    source.set(vec![tenant_binding("acme", 2, "shared-01")]);
    handle.refresh_now();

    let change = changes.recv().await.unwrap();
    assert_eq!(change.current_revision, Some(BindingRevision::new(2)));

    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_failed_refresh_keeps_the_last_good_snapshot() {
    // The behaviour that matters most: an unreadable source must not empty the
    // registry and take every tenant down with it.
    let registry = Arc::new(TenantRegistry::new());
    let source = Arc::new(InMemorySource::new(vec![tenant_binding("acme", 5, "shared-01")]));

    ResourceRefresher::prime(&registry, source.as_ref())
        .await
        .unwrap();

    let handle = ResourceRefresher::spawn(
        Arc::clone(&registry),
        Arc::clone(&source) as Arc<dyn ResourceSource<TenantRuntimeBinding>>,
        &config(),
    );

    source.fail_next();
    handle.refresh_now();

    // Give the background task a turn, then a second refresh to prove the loop
    // survived the failure.
    tokio::task::yield_now().await;
    handle.refresh_now();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(registry.is_primed());
    assert_eq!(
        registry.lookup(&tenant("acme")).unwrap().revision,
        BindingRevision::new(5)
    );

    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_stops_the_loop() {
    let registry = Arc::new(TenantRegistry::new());
    let source = Arc::new(InMemorySource::empty()) as Arc<dyn ResourceSource<TenantRuntimeBinding>>;

    let handle = ResourceRefresher::spawn(registry, source, &config());

    assert!(handle.shutdown().await.is_ok());
}
