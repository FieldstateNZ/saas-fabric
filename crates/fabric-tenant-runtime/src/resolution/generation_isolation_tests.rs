//! A tenant's execution target always comes from one complete generation.
//!
//! The scenario (review of #136): two dedicated tenants, each on its own
//! database, swap DataSources in one generation while both DataSources swap
//! the databases they select. Each tenant stays on its own database in both
//! generations, but the old tenant bindings paired with the new DataSources
//! resolve each tenant to the *other* tenant's database -- a successful,
//! cross-tenant execution target. The refresher writes data sources before
//! tenants, so that pairing exists in the registries between the two writes.
//!
//! The assertions compare the whole `ExecutionTarget` -- tenant, DataSource,
//! both revisions, connector, connection and isolation -- not only the
//! DataSource id.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use fabric_connector::{ConnectorId, ExecutionTarget, IsolationModel};
use fabric_core::BindingRevision;

use crate::publication::{PublishedState, PublishedStateSource};
use crate::resolution::PublishedGeneration;
use crate::testing::{connection_for, data_source, data_source_id, primary, tenant, tenant_binding};
use crate::{
    build_published_runtime, DataSource, DataSourceRegistry, RuntimeConfig, RuntimeResolver, SourceError,
    TenantRegistry, TenantRuntimeBinding,
};

/// A dedicated DataSource selecting the named physical database.
fn on_database(id: &str, revision: u64, database: &str) -> DataSource {
    DataSource {
        connection: connection_for(database),
        ..data_source(id, revision)
    }
}

/// Generation `revision`: every resource at that revision. Odd revisions
/// put acme on source-a and globex on source-b, each DataSource on its own
/// tenant's database. Even revisions swap both the tenants' DataSources and
/// the DataSources' databases, so each tenant stays on its own database.
fn generation(revision: u64) -> (Vec<DataSource>, Vec<TenantRuntimeBinding>) {
    let swapped = revision.is_multiple_of(2);
    let (acme_source, globex_source) = if swapped {
        ("source-b", "source-a")
    } else {
        ("source-a", "source-b")
    };
    (
        vec![
            on_database(acme_source, revision, "database-a"),
            on_database(globex_source, revision, "database-b"),
        ],
        vec![
            tenant_binding("acme", revision, acme_source),
            tenant_binding("globex", revision, globex_source),
        ],
    )
}

/// The target `name` must resolve to in generation `revision`.
fn expected(name: &str, revision: u64) -> ExecutionTarget {
    let swapped = revision.is_multiple_of(2);
    let (source, database) = match (name, swapped) {
        ("acme", false) => ("source-a", "database-a"),
        ("acme", true) => ("source-b", "database-a"),
        (_, false) => ("source-b", "database-b"),
        (_, true) => ("source-a", "database-b"),
    };
    ExecutionTarget::new(
        tenant(name),
        BindingRevision::new(revision),
        data_source_id(source),
        BindingRevision::new(revision),
        ConnectorId::try_new("postgres").unwrap(),
        connection_for(database),
        IsolationModel::Database,
    )
}

/// The generation `name`'s whole target belongs to, or the target it
/// resolved to instead when no single generation produced it.
fn generation_of(resolver: &RuntimeResolver, name: &str) -> Result<u64, String> {
    let target = resolver
        .resolve_data_source(&tenant(name), &primary())
        .map_err(|error| format!("{name} did not resolve: {error}"))?
        .target;
    let revision = target.tenant_revision().get();
    if target == expected(name, revision) {
        Ok(revision)
    } else {
        Err(format!(
            "{name} resolved to a target from no single generation: {target:?}"
        ))
    }
}

fn both_resolve_to(resolver: &RuntimeResolver, revision: u64) {
    assert_eq!(generation_of(resolver, "acme"), Ok(revision));
    assert_eq!(generation_of(resolver, "globex"), Ok(revision));
}

fn primed_on_the_old_generation() -> (
    Arc<PublishedGeneration>,
    Arc<TenantRegistry>,
    Arc<DataSourceRegistry>,
) {
    let (sources, tenants) = generation(1);
    let (published, tenant_registry, source_registry) = (
        Arc::new(PublishedGeneration::default()),
        Arc::new(TenantRegistry::new()),
        Arc::new(DataSourceRegistry::new()),
    );
    source_registry.apply_all(sources).unwrap();
    tenant_registry.apply_all(tenants).unwrap();
    published.publish(&tenant_registry, &source_registry);
    (published, tenant_registry, source_registry)
}

/// Steps through exactly the writes the refresher makes for one generation
/// -- data sources, then tenants, then one publish -- resolving at every
/// point between them.
#[test]
fn a_tenant_resolves_to_one_complete_generations_target_at_every_point_of_a_refresh() {
    let (published, tenants, sources) = primed_on_the_old_generation();
    let resolver =
        RuntimeResolver::over_published(Arc::clone(&tenants), Arc::clone(&sources), Arc::clone(&published));
    both_resolve_to(&resolver, 1);

    let (new_sources, new_tenants) = generation(2);
    sources.apply_all(new_sources).unwrap();
    both_resolve_to(&resolver, 1);

    tenants.apply_all(new_tenants).unwrap();
    both_resolve_to(&resolver, 1);

    published.publish(&tenants, &sources);
    both_resolve_to(&resolver, 2);
}

/// What the registries hold between the refresher's two writes, resolved
/// across them directly -- as `build_runtime`'s resolver, over two
/// independent sources, still does. Production (`fabric-api`) resolves
/// through `build_published_runtime` and never reaches this pairing; the
/// test pins that it is a cross-tenant target, which is why it must not.
#[test]
fn current_behaviour_resolving_across_the_registries_between_the_two_writes_reaches_the_other_tenants_database(
) {
    let (_published, tenants, sources) = primed_on_the_old_generation();
    sources.apply_all(generation(2).0).unwrap();

    let resolver = RuntimeResolver::new(tenants, sources);
    let acme = resolver
        .resolve_data_source(&tenant("acme"), &primary())
        .unwrap()
        .target;

    assert_eq!(acme.connection(), &connection_for("database-b"));
    assert_eq!(
        (acme.tenant_revision(), acme.data_source_revision()),
        (BindingRevision::new(1), BindingRevision::new(2))
    );
}

/// Serves generation `current`, which the test advances.
struct Advancing {
    current: AtomicU64,
}

#[async_trait]
impl PublishedStateSource for Advancing {
    async fn load(&self) -> Result<PublishedState, SourceError> {
        let number = self.current.load(Ordering::SeqCst);
        let (data_sources, tenants) = generation(number);
        Ok(PublishedState::Generation {
            generation: number,
            data_sources,
            tenants,
        })
    }

    fn describe(&self) -> String {
        "advancing".to_owned()
    }
}

/// The same scenario through the production builder and its refresher, with
/// a thread resolving continuously while the generation advances, each step
/// swapping every binding. Supplements the deterministic test above, which
/// is the one that reaches the point between the two writes on purpose.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sampler_never_sees_a_mixed_target_while_the_published_generation_advances() {
    let source = Arc::new(Advancing {
        current: AtomicU64::new(1),
    });
    let config = RuntimeConfig {
        refresh_interval_seconds: 1,
        fail_fast_on_prime: true,
    };
    let crate::PublishedRuntime {
        resolver, handles, ..
    } = build_published_runtime(&config, Arc::clone(&source) as Arc<dyn PublishedStateSource>)
        .await
        .unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (resolver, stop) = (Arc::clone(&resolver), Arc::clone(&stop));
        std::thread::spawn(move || {
            let mut samples = 0_u64;
            while !stop.load(Ordering::SeqCst) {
                for name in ["acme", "globex"] {
                    generation_of(&resolver, name)?;
                }
                samples += 1;
            }
            Ok::<u64, String>(samples)
        })
    };

    for next in 2..=21 {
        source.current.store(next, Ordering::SeqCst);
        handles.refresh_now();
        let deadline = Instant::now() + Duration::from_secs(5);
        while generation_of(&resolver, "acme") != Ok(next) {
            assert!(
                Instant::now() < deadline,
                "the refresher never applied generation {next}"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    stop.store(true, Ordering::SeqCst);
    let resolutions = sampler.join().unwrap().unwrap();
    assert!(resolutions > 0);
    handles.shutdown().await.unwrap();
}
