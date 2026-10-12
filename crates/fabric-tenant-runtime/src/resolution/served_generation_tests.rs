//! A replica reports exactly the generation its resolution reads (G7).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use fabric_core::{ReaderGenerationReport, RecordDigest, ServedOrigin};

use crate::publication::{GenerationOrigin, PublishedState, PublishedStateSource};
use crate::resolution::PublishedGeneration;
use crate::testing::{data_source, primary, tenant, tenant_binding};
use crate::{
    build_published_runtime, DataSource, DataSourceRegistry, RuntimeConfig, RuntimeResolver, SourceError,
    TenantRegistry, TenantRuntimeBinding,
};

fn digest(generation: u64) -> RecordDigest {
    RecordDigest::from_bytes([u8::try_from(generation).unwrap(); 32])
}

fn stamp(generation: u64, origin: ServedOrigin) -> ReaderGenerationReport {
    ReaderGenerationReport::new(generation, digest(generation), origin)
}

/// Every resource at `revision`, acme bound to `bound_to`.
fn documents(revision: u64, bound_to: &str) -> (Vec<DataSource>, Vec<TenantRuntimeBinding>) {
    (
        vec![
            data_source("source-a", revision),
            data_source("source-b", revision),
        ],
        vec![tenant_binding("acme", revision, bound_to)],
    )
}

fn acme_revision(resolver: &RuntimeResolver) -> u64 {
    resolver
        .resolve_data_source(&tenant("acme"), &primary())
        .unwrap()
        .target
        .tenant_revision()
        .get()
}

/// Steps through the refresher's writes for one generation. The report
/// moves only with the view resolution reads, never with a registry write.
#[test]
fn the_report_moves_in_the_same_store_as_the_view_resolution_reads() {
    let (published, tenants, sources) = (
        Arc::new(PublishedGeneration::default()),
        Arc::new(TenantRegistry::new()),
        Arc::new(DataSourceRegistry::new()),
    );
    let resolver =
        RuntimeResolver::over_published(Arc::clone(&tenants), Arc::clone(&sources), Arc::clone(&published));
    assert_eq!(resolver.served_generation(), None, "nothing installed");

    let (first_sources, first_tenants) = documents(1, "source-a");
    sources.apply_all(first_sources).unwrap();
    tenants.apply_all(first_tenants).unwrap();
    published.publish(&tenants, &sources, Some(stamp(1, ServedOrigin::Published)));
    assert_eq!(
        resolver.served_generation(),
        Some(stamp(1, ServedOrigin::Published))
    );

    let (next_sources, next_tenants) = documents(2, "source-b");
    sources.apply_all(next_sources).unwrap();
    assert_eq!(
        resolver.served_generation(),
        Some(stamp(1, ServedOrigin::Published))
    );
    assert_eq!(acme_revision(&resolver), 1);
    tenants.apply_all(next_tenants).unwrap();
    assert_eq!(
        resolver.served_generation(),
        Some(stamp(1, ServedOrigin::Published))
    );
    assert_eq!(acme_revision(&resolver), 1);

    published.publish(&tenants, &sources, Some(stamp(2, ServedOrigin::Published)));
    assert_eq!(
        resolver.served_generation(),
        Some(stamp(2, ServedOrigin::Published))
    );
    assert_eq!(acme_revision(&resolver), 2);
}

#[test]
fn a_resolver_over_independent_sources_reports_nothing() {
    let resolver = RuntimeResolver::new(
        Arc::new(TenantRegistry::new()),
        Arc::new(DataSourceRegistry::new()),
    );
    assert_eq!(resolver.served_generation(), None);
}

/// One scripted read per step: what the source answers at each step.
struct Scripted {
    step: AtomicU64,
}

#[async_trait]
impl PublishedStateSource for Scripted {
    async fn load(&self) -> Result<PublishedState, SourceError> {
        let generation = |number: u64, origin: GenerationOrigin, (data_sources, tenants)| {
            Ok(PublishedState::Generation {
                generation: number,
                record_sha256: digest(number),
                origin,
                data_sources,
                tenants,
            })
        };
        match self.step.load(Ordering::SeqCst) {
            0 => Err(SourceError::Unreadable {
                origin: "scripted".to_owned(),
                cause: "not yet".into(),
            }),
            1 => generation(1, GenerationOrigin::Published, documents(1, "source-a")),
            // Same revision, another binding: the registry keeps its copy of
            // acme, so the view is not generation 2.
            2 => generation(2, GenerationOrigin::Published, documents(1, "source-b")),
            3 => generation(
                3,
                GenerationOrigin::LastKnownGood {
                    held_because: "torn".to_owned(),
                },
                documents(3, "source-a"),
            ),
            4 => generation(4, GenerationOrigin::Published, documents(4, "source-b")),
            _ => Ok(PublishedState::Unrecorded {
                data_sources: Ok(documents(5, "source-a").0),
                tenants: Ok(documents(5, "source-a").1),
            }),
        }
    }

    fn describe(&self) -> String {
        "scripted".to_owned()
    }
}

async fn until(resolver: &RuntimeResolver, wanted: Option<ReaderGenerationReport>, acme: u64) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while resolver.served_generation() != wanted || acme_revision(resolver) != acme {
        assert!(
            Instant::now() < deadline,
            "never reached {wanted:?} at acme revision {acme}"
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

#[tokio::test]
async fn the_report_follows_every_kind_of_read_through_the_production_builder() {
    let source = Arc::new(Scripted {
        step: AtomicU64::new(0),
    });
    let config = RuntimeConfig {
        refresh_interval_seconds: 60,
        fail_fast_on_prime: false,
        ..RuntimeConfig::default()
    };
    let runtime = build_published_runtime(&config, Arc::clone(&source) as Arc<dyn PublishedStateSource>)
        .await
        .unwrap();
    let resolver = runtime.resolver;
    assert_eq!(
        resolver.served_generation(),
        None,
        "an unprimed replica serves nothing"
    );

    let advance = |step: u64| {
        source.step.store(step, Ordering::SeqCst);
        runtime.handles.refresh_now();
    };
    advance(1);
    until(&resolver, Some(stamp(1, ServedOrigin::Published)), 1).await;

    advance(2);
    let deadline = Instant::now() + Duration::from_secs(5);
    while resolver.served_generation().is_some() {
        assert!(
            Instant::now() < deadline,
            "a partly taken generation was reported"
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(acme_revision(&resolver), 1, "the held copy still serves");

    advance(3);
    until(&resolver, Some(stamp(3, ServedOrigin::LastKnownGood)), 3).await;

    advance(4);
    until(&resolver, Some(stamp(4, ServedOrigin::Published)), 4).await;

    advance(5);
    until(&resolver, None, 5).await;

    runtime.handles.shutdown().await.unwrap();
}
