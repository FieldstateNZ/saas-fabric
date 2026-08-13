//! Item 14: the priming-order guarantee that `build_runtime` documents —
//! DataSources load before tenant bindings, so a binding referencing a
//! DataSource that has not loaded yet never produces a spurious
//! `MissingDataSource` in the first moments after startup.
//!
//! Also the readiness guarantee the order exists to protect: a first load that
//! installs nothing must not leave the process reporting ready.

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use fabric_core::{BindingRevision, TenantId};

use crate::resource::sources::InMemorySource;
use crate::resource::{RegistryResource, ResourceSource};
use crate::testing::{data_source, tenant_binding};
use crate::{build_runtime, DataSource, PoolSettings, RuntimeConfig, SourceError, TenantRuntimeBinding};

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

#[tokio::test]
async fn without_fail_fast_a_repeated_key_still_never_starts_primed_and_empty() {
    // `fail_fast_on_prime: false` is allowed to start anyway; it is not allowed
    // to start *primed and empty*. Unprimed answers 503 and keeps the replica
    // out of the load balancer.
    let config = RuntimeConfig {
        fail_fast_on_prime: false,
        ..RuntimeConfig::default()
    };
    let data_sources = Arc::new(InMemorySource::new(vec![data_source("shared-01", 1)]));
    let tenants = Arc::new(InMemorySource::new(vec![
        unusable_binding("orphan"),
        unusable_binding("stray"),
    ]));

    let (resolver, handles) = build_runtime(&config, tenants, data_sources).await.unwrap();

    assert!(!resolver.is_primed());

    handles.shutdown().await.unwrap();
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

#[tokio::test]
async fn without_fail_fast_an_unusable_first_load_starts_unprimed_rather_than_empty() {
    // `fail_fast_on_prime: false` chooses to start anyway. What it must not
    // choose is to start *primed and empty*: unprimed returns 503 and keeps
    // the replica out of the load balancer, primed-and-empty returns 500 while
    // /ready insists the replica is healthy.
    let config = RuntimeConfig {
        fail_fast_on_prime: false,
        ..RuntimeConfig::default()
    };
    let data_sources = Arc::new(InMemorySource::new(vec![data_source("shared-01", 1)]));
    let tenants = Arc::new(InMemorySource::new(vec![unusable_binding("orphan")]));

    let (resolver, handles) = build_runtime(&config, tenants, data_sources).await.unwrap();

    assert!(!resolver.is_primed());

    handles.shutdown().await.unwrap();
}
