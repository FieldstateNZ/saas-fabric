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

/// A binding with no data bindings at all — the rule
/// [`TenantRuntimeBinding::validate`] enforces. Every data request for it
/// would fail, so it can never be served.
fn unusable_binding(name: &str, revision: u64) -> TenantRuntimeBinding {
    TenantRuntimeBinding::new(tenant(name), BindingRevision::new(revision))
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
async fn a_first_load_that_can_install_nothing_is_a_load_failure() {
    // The source read cleanly, so no `SourceError` fires — but every entry
    // fails validation and there is no previously-held copy to fall back on,
    // so the snapshot installed is empty. A registry in that state reports
    // primed, answers /ready with 200, and returns `MissingDataSource` — a
    // 500 — for every request that touches it.
    let registry = TenantRegistry::new();
    let source = InMemorySource::new(vec![unusable_binding("acme", 1)]);

    assert!(ResourceRefresher::prime(&registry, &source).await.is_err());
    assert!(
        !registry.is_primed(),
        "a refused prime must leave the registry unprimed, or /ready still answers 200"
    );
}

#[tokio::test]
async fn a_partly_invalid_first_load_still_serves_what_is_usable() {
    // One rejected out of two is not the same as two out of two. There is
    // something to serve, so `is_primed` is honest — and refusing to start
    // here would take every healthy tenant offline over one operator's typo,
    // which is the failure the drop-with-a-log rule exists to prevent.
    let registry = TenantRegistry::new();
    let source = InMemorySource::new(vec![
        unusable_binding("orphan", 1),
        tenant_binding("acme", 1, "shared-01"),
    ]);

    let count = ResourceRefresher::prime(&registry, &source).await.unwrap();

    assert_eq!(count, 1);
    assert!(registry.is_primed());
    assert!(registry.lookup(&tenant("acme")).is_ok());
}

#[tokio::test]
async fn a_first_load_repeating_a_key_serves_the_entry_that_is_usable() {
    // The shape `JsonFileSource` makes reachable — a JSON array with no
    // duplicate-key check — and the one the prime guard and the merge used to
    // answer differently. The guard saw a usable set and waved it through; the
    // merge dropped the first entry as invalid and the second as a duplicate of
    // it. `prime` returned `Ok`, so `fail_fast_on_prime: true` never fired, and
    // the process came up primed over zero tenants.
    let registry = TenantRegistry::new();
    let source = InMemorySource::new(vec![
        unusable_binding("acme", 1),
        tenant_binding("acme", 2, "shared-01"),
    ]);

    let count = ResourceRefresher::prime(&registry, &source).await.unwrap();

    assert_eq!(
        count, 1,
        "a valid entry for acme was published, so one must be installed"
    );
    assert!(registry.is_primed());
    assert_eq!(
        registry.lookup(&tenant("acme")).unwrap().revision,
        BindingRevision::new(2)
    );
}

#[tokio::test]
async fn a_refresh_installs_a_new_tenant_whose_first_entry_is_unusable() {
    // The same root cause on the refresh path, where it is less severe only
    // because the registry is already serving: `acme` has no held copy, so
    // nothing was retained for it and the usable entry was refused as a
    // duplicate. The tenant simply never appeared.
    let registry = TenantRegistry::new();
    let source = InMemorySource::new(vec![tenant_binding("incumbent", 1, "shared-01")]);
    ResourceRefresher::prime(&registry, &source).await.unwrap();

    registry
        .apply_all(vec![
            tenant_binding("incumbent", 1, "shared-01"),
            unusable_binding("acme", 1),
            tenant_binding("acme", 2, "shared-01"),
        ])
        .unwrap();

    assert!(registry.lookup(&tenant("acme")).is_ok());
    assert!(registry.lookup(&tenant("incumbent")).is_ok());
}

#[tokio::test]
async fn a_genuinely_empty_source_primes_successfully() {
    // A deployment that has not onboarded a tenant yet must still start.
    // Installing nothing is only a failure when the source actually published
    // something to install.
    let registry = TenantRegistry::new();
    let source: InMemorySource<TenantRuntimeBinding> = InMemorySource::empty();

    assert_eq!(ResourceRefresher::prime(&registry, &source).await.unwrap(), 0);
    assert!(registry.is_primed());
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

#[tokio::test(start_paused = true)]
async fn a_failed_refresh_keeps_the_last_good_snapshot() {
    // The behaviour that matters most: an unreadable source must not empty the
    // registry and take every tenant down with it.
    //
    // # Why the shape of this test is load-bearing
    //
    // It used to trigger a refresh, then trigger a *second* one — and the
    // second, successful load reinstalled everything the first was supposed to
    // have preserved. With `apply_all(Vec::new())` injected into the failure
    // arm of the refresh loop, the exact defect this test names, it passed.
    // So did all 126 of its neighbours. The single most important refresher
    // invariant had no regression coverage at all.
    //
    // Two things fix that. Nothing repairs the damage before the assertions
    // run, and the source's load counter proves the failing refresh actually
    // happened — otherwise "the registry still holds acme" is just the
    // precondition restated, and passes whether or not the loop ever ran.
    let registry = Arc::new(TenantRegistry::new());
    let source = Arc::new(InMemorySource::new(vec![
        tenant_binding("acme", 5, "shared-01"),
        tenant_binding("globex", 2, "globex-01"),
    ]));

    ResourceRefresher::prime(&registry, source.as_ref())
        .await
        .unwrap();
    let loads_after_prime = source.loads();

    let handle = ResourceRefresher::spawn(
        Arc::clone(&registry),
        Arc::clone(&source) as Arc<dyn ResourceSource<TenantRuntimeBinding>>,
        &config(),
    );

    source.fail_next();
    handle.refresh_now();

    // The clock is paused, so this yields to the refresh task and then jumps
    // straight to this deadline — well short of the hour-long poll interval,
    // which means exactly one load happens and it is the failing one.
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(
        source.loads(),
        loads_after_prime + 1,
        "the refresh loop never re-read the source, so this test proves nothing"
    );

    assert!(
        registry.is_primed(),
        "a failed load must not un-prime the registry"
    );
    assert_eq!(
        registry.len(),
        2,
        "a momentarily unreadable source must not deprovision anything"
    );
    assert_eq!(
        registry.lookup(&tenant("acme")).unwrap().revision,
        BindingRevision::new(5)
    );
    assert_eq!(
        registry.lookup(&tenant("globex")).unwrap().revision,
        BindingRevision::new(2)
    );

    // Only now, after the invariant has been judged, prove the loop survived
    // the failure — by observing something the previous snapshot did not have.
    source.set(vec![tenant_binding("acme", 6, "shared-01")]);
    handle.refresh_now();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(
        registry.lookup(&tenant("acme")).unwrap().revision,
        BindingRevision::new(6),
        "the loop must keep running after a failed load"
    );

    handle.shutdown().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn shutdown_stops_the_loop() {
    // `is_ok()` on its own proved nothing about the name of this test. The
    // task cannot panic over an empty in-memory source, so the result was
    // `Ok` by construction — and the two ways the loop could fail to stop are
    // worse than a failure: without the `break` arm, or without the notify,
    // `self.task.await` never resolves and this hangs.
    //
    // So the loop is made to tick first, and what is asserted is that it stops
    // ticking. That fails rather than hangs, and it fails for a detached task
    // as well as a running one.
    let registry = Arc::new(TenantRegistry::new());
    let source: Arc<InMemorySource<TenantRuntimeBinding>> = Arc::new(InMemorySource::empty());
    let config = RuntimeConfig {
        refresh_interval_seconds: 1,
        fail_fast_on_prime: true,
    };

    let handle = ResourceRefresher::spawn(
        registry,
        Arc::clone(&source) as Arc<dyn ResourceSource<TenantRuntimeBinding>>,
        &config,
    );

    tokio::time::sleep(Duration::from_secs(1) + Duration::from_millis(500)).await;
    let loads_while_running = source.loads();
    assert!(
        loads_while_running > 0,
        "precondition: the loop never polled, so stopping it proves nothing"
    );

    assert!(handle.shutdown().await.is_ok());

    tokio::time::sleep(Duration::from_secs(3) + Duration::from_millis(500)).await;
    assert_eq!(
        source.loads(),
        loads_while_running,
        "the loop kept polling after shutdown returned"
    );
}
