//! Item 14: the priming-order guarantee that `build_runtime` documents —
//! DataSources load before tenant bindings, so a binding referencing a
//! DataSource that has not loaded yet never produces a spurious
//! `MissingDataSource` in the first moments after startup.
//!
//! Also the readiness guarantee the order exists to protect: a first load that
//! installs nothing must not leave the process reporting ready.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use fabric_core::{BindingRevision, TenantId};

use crate::resource::sources::InMemorySource;
use crate::resource::{RegistryResource, ResourceSource};
use crate::testing::{data_source, tenant_binding};
use crate::{
    build_runtime, DataSource, PoolSettings, ResolveError, RuntimeConfig, RuntimeResolver, SourceError,
    TenantRuntimeBinding,
};

/// A source that records when it was asked to load, so the test can assert
/// on the order two independent sources were primed in.
struct OrderRecordingSource<T> {
    resources: Vec<T>,
    order: Arc<Mutex<Vec<&'static str>>>,
    label: &'static str,
}

#[async_trait]
impl<T: RegistryResource> ResourceSource<T> for OrderRecordingSource<T> {
    async fn load(&self) -> Result<Vec<T>, SourceError> {
        self.order
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(self.label);

        Ok(self.resources.clone())
    }

    fn describe(&self) -> String {
        self.label.to_owned()
    }
}

#[tokio::test]
async fn data_sources_prime_before_tenant_bindings() {
    let order = Arc::new(Mutex::new(Vec::new()));

    let data_source_source: Arc<OrderRecordingSource<DataSource>> = Arc::new(OrderRecordingSource {
        resources: vec![data_source("shared-01", 1)],
        order: Arc::clone(&order),
        label: "data_sources",
    });
    let tenant_source: Arc<OrderRecordingSource<TenantRuntimeBinding>> = Arc::new(OrderRecordingSource {
        resources: vec![tenant_binding("acme", 1, "shared-01")],
        order: Arc::clone(&order),
        label: "tenants",
    });

    let (_resolver, handles) = build_runtime(&RuntimeConfig::default(), tenant_source, data_source_source)
        .await
        .unwrap();

    handles.shutdown().await.unwrap();

    let order = order.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        order.as_slice(),
        ["data_sources", "tenants"],
        "a tenant binding referencing a DataSource must never be primed before that DataSource, \
         or it would resolve to a spurious MissingDataSource in the first moments after startup"
    );
}

/// A binding with no data bindings — unusable, and unusable for a reason this
/// crate genuinely acts on: every data request for it would fail.
fn unusable_binding(name: &str) -> TenantRuntimeBinding {
    TenantRuntimeBinding::new(TenantId::try_new(name).unwrap(), BindingRevision::new(1))
}

/// How many refresh intervals [`survive_the_refresh_loop`] lets elapse.
const INTERVALS_TO_OUTLAST: u64 = 3;

/// Starts the runtime over `tenants` with `fail_fast_on_prime: false`, lets the
/// background refreshers actually run, stops them, and reports the resolver plus
/// how many times the tenant source was loaded.
///
/// # Why this exists
///
/// The guarantee it serves is timing-dependent: the hole it closes opens one
/// refresh interval after startup, not at startup. Two tests here used to assert
/// `!is_primed()` at t=0 and shut down immediately, and with the default 30
/// second interval the loop never fired inside them — so a refresh loop that
/// primed the registry over an empty snapshot sailed past both of them.
///
/// # Why the clock is paused rather than short
///
/// A real one-second interval and a real sleep would only make the loop
/// *likely* to have run. That flake direction is the dangerous one: on a loaded
/// machine the timer would not fire, `is_primed()` would still be false, and the
/// test would pass while the bug was present. Tokio's paused clock advances to
/// the next deadline whenever every task is idle, so the loop is *guaranteed* to
/// have run — deterministically, and in no wall-clock time.
///
/// `loads` is returned for the same reason: callers assert the source really was
/// re-read, so a future change that stops the loop running fails loudly here
/// instead of quietly making these tests vacuous again.
async fn survive_the_refresh_loop(tenants: Vec<TenantRuntimeBinding>) -> (Arc<RuntimeResolver>, usize) {
    let order = Arc::new(Mutex::new(Vec::new()));
    let config = RuntimeConfig {
        fail_fast_on_prime: false,
        refresh_interval_seconds: 1,
    };

    let data_source_source: Arc<OrderRecordingSource<DataSource>> = Arc::new(OrderRecordingSource {
        resources: vec![data_source("shared-01", 1)],
        order: Arc::clone(&order),
        label: "data_sources",
    });
    let tenant_source: Arc<OrderRecordingSource<TenantRuntimeBinding>> = Arc::new(OrderRecordingSource {
        resources: tenants,
        order: Arc::clone(&order),
        label: "tenants",
    });

    let (resolver, handles) = build_runtime(&config, tenant_source, data_source_source)
        .await
        .unwrap();
    assert!(
        !resolver.is_primed(),
        "precondition: the prime must have been refused"
    );

    tokio::time::sleep(std::time::Duration::from_secs(INTERVALS_TO_OUTLAST) + Duration::from_millis(500))
        .await;
    handles.shutdown().await.unwrap();

    let loads = order
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|label| **label == "tenants")
        .count();

    (resolver, loads)
}

#[tokio::test]
async fn a_first_load_that_installs_nothing_refuses_to_start() {
    // The reviewer's reproduction, retargeted at the tenant registry now that
    // a DataSource has no way to be invalid. The shape is the one that
    // mattered: the source parses cleanly, so no `SourceError` fires and
    // `fail_fast_on_prime` never gets a say — but the one entry fails
    // validation, so the registry primes empty. `build_runtime` returned `Ok`,
    // `is_primed()` was true, /ready answered 200, and every request 500d.
    let data_sources = Arc::new(InMemorySource::new(vec![data_source("shared-01", 1)]));
    let tenants = Arc::new(InMemorySource::new(vec![unusable_binding("orphan")]));

    let result = build_runtime(&RuntimeConfig::default(), tenants, data_sources).await;

    assert!(
        result.is_err(),
        "a first load whose every entry was rejected installs nothing, which is the empty \
         set a load failure must never become"
    );
}

#[tokio::test]
async fn a_repeated_tenant_key_does_not_start_a_replica_over_zero_tenants() {
    // End to end, the shape that reopened the hole: the source publishes an
    // unusable entry for `acme` followed by a usable one. `build_runtime`
    // returned `Ok` with a primed, empty tenant registry — /ready 200, every
    // request 403 or 500, and `fail_fast_on_prime: true` with nothing to fire
    // on because the load never reported a failure.
    let data_sources = Arc::new(InMemorySource::new(vec![data_source("shared-01", 1)]));
    let tenants = Arc::new(InMemorySource::new(vec![
        unusable_binding("acme"),
        tenant_binding("acme", 2, "shared-01"),
    ]));

    let (resolver, handles) = build_runtime(&RuntimeConfig::default(), tenants, data_sources)
        .await
        .unwrap();

    assert!(resolver.is_primed());
    assert!(
        resolver
            .resolve_data_source(&TenantId::try_new("acme").unwrap(), &crate::testing::primary())
            .is_ok(),
        "a usable binding for acme was published, so the replica must be able to serve it"
    );

    handles.shutdown().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn without_fail_fast_two_unusable_bindings_never_leave_the_replica_primed_and_empty() {
    // `fail_fast_on_prime: false` is allowed to start anyway; it is not allowed
    // to start *primed and empty*. Unprimed answers 503 and keeps the replica
    // out of the load balancer.
    let (resolver, loads) =
        survive_the_refresh_loop(vec![unusable_binding("orphan"), unusable_binding("stray")]).await;

    assert!(
        loads > 1,
        "the refresh loop never re-read the source ({loads} loads), so this test proves nothing"
    );
    assert!(!resolver.is_primed());
}

#[tokio::test]
async fn an_incoherent_pool_no_longer_takes_the_whole_replica_down() {
    // The other half of the reviewer's scenario. `max_connections: 0` used to
    // keep the DataSource out of the registry; with the prime guard in place
    // that would have escalated from "500s for the tenants on it" to "the
    // replica refuses to boot" — over a field nothing reads.
    let data_sources = Arc::new(InMemorySource::new(vec![DataSource {
        pool: PoolSettings {
            max_connections: 0,
            ..PoolSettings::default()
        },
        ..data_source("shared-01", 1)
    }]));
    let tenants = Arc::new(InMemorySource::new(vec![tenant_binding("acme", 1, "shared-01")]));

    let (resolver, handles) = build_runtime(&RuntimeConfig::default(), tenants, data_sources)
        .await
        .unwrap();

    assert!(resolver.is_primed());
    assert!(resolver
        .resolve_data_source(&TenantId::try_new("acme").unwrap(), &crate::testing::primary())
        .is_ok());

    handles.shutdown().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn without_fail_fast_an_unusable_first_load_stays_unprimed_across_refreshes() {
    // `fail_fast_on_prime: false` chooses to start anyway. What it must not
    // choose is to start *primed and empty*: unprimed returns 503 and keeps the
    // replica out of the load balancer, primed-and-empty puts it in while every
    // request fails.
    //
    // The refusal has to survive the background refresh loop, which is where it
    // used to be undone. The source never changes — the same unusable payload
    // that got the prime refused is what each refresh re-reads — so nothing but
    // the registry's own rule stands between here and a primed, empty replica.
    let (resolver, loads) = survive_the_refresh_loop(vec![unusable_binding("orphan")]).await;

    assert!(
        loads > 1,
        "the refresh loop never re-read the source ({loads} loads), so this test proves nothing"
    );
    assert!(
        !resolver.is_primed(),
        "primed over an empty snapshot {INTERVALS_TO_OUTLAST} refresh intervals after a refused prime"
    );
}

#[tokio::test(start_paused = true)]
async fn a_refused_prime_keeps_answering_retryably_rather_than_denying_the_tenant() {
    // What the flip actually did to callers, and the reason it is worse than it
    // sounds. `RuntimeUnavailable` is a 503: honest, retryable, and it keeps the
    // replica out of the load balancer. `UnknownTenant` is a 403 telling a
    // caller their tenant does not exist — terminal, and wrong.
    let (resolver, _loads) = survive_the_refresh_loop(vec![unusable_binding("orphan")]).await;

    let error = resolver
        .resolve_data_source(&TenantId::try_new("orphan").unwrap(), &crate::testing::primary())
        .unwrap_err();

    assert!(
        matches!(error, ResolveError::RuntimeUnavailable),
        "a replica that never primed must answer 503 runtime_unavailable, not deny the tenant: {error:?}"
    );
}
