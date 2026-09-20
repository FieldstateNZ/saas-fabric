//! `RuntimePublisher::publish_once`: the input-to-outcome mapping D3 names,
//! the re-entry guard, and that the guard is released whatever the pass
//! decided.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_core::{
    BindingRevision, Clock, DataSourceId, LogicalDataSourceName, LogicalResourceName, OperationKind,
    SystemClock, TenantId,
};
use fabric_runtime_publication::{
    CatalogDocument, CollectionName, ConnectionName, ConnectionSelectorDocument, ConnectorId,
    DataResidencyDocument, DataSourceCapabilitiesDocument, FieldName, IsolationModelDocument,
    PlacementClassDocument, PoolSettingsDocument, PublicationError, PublicationReport, PublishedRevisions,
    ResourceDefinitionDocument, RuntimePublication, RuntimeSnapshot,
};
use tokio::sync::Notify;

use crate::publication::catalogue_source::CatalogueSourceError;
use crate::publication::testing::FakePublication;
use crate::{
    ComponentDesired, DataSourceDeclaration, DataSourceState, DataSourcesRead, DesiredRevision, DesiredState,
    DesiredStateError, EnvironmentWrite, Hold, PassOutcome, PassResult, PlacementRecord, PlacementState,
    PlacementsRead, PlatformRepository, PublicationState, Release, RuntimeCatalogueSource, RuntimePublisher,
    WaitingReason,
};

fn declaration(id: &str) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(id).unwrap(),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres-nz").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").unwrap(),
        },
        placement: PlacementClassDocument::Dedicated,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument::default(),
        discriminator: None,
        labels: BTreeMap::new(),
    }
}

fn placement(tenant: &str, data_source: &str) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(tenant).unwrap(),
        logical: LogicalDataSourceName::try_new("primary").unwrap(),
        revision: BindingRevision::new(1),
        data_source: DataSourceId::try_new(data_source).unwrap(),
        isolation: IsolationModelDocument::Database {},
        placed_at: "2026-09-18T02:14:00Z".to_owned(),
    }
}

fn catalog_with_one_resource() -> CatalogDocument {
    let mut resources = BTreeMap::new();
    resources.insert(
        LogicalResourceName::try_new("customers").unwrap(),
        ResourceDefinitionDocument {
            data_source: LogicalDataSourceName::try_new("primary").unwrap(),
            collection: CollectionName::try_new("customers").unwrap(),
            key_field: FieldName::try_new("id").unwrap(),
            operations: vec![OperationKind::Read],
            queryable_fields: Vec::new(),
        },
    );
    CatalogDocument::new(resources)
}

/// Answers every port a [`PlatformRepository`] needs; only `read_*` is
/// exercised by a publication pass, so every write and every `DesiredState`
/// method panics if reached.
struct FakePlatform {
    data_sources: Result<DataSourcesRead, DesiredStateError>,
    placements: Result<PlacementsRead, DesiredStateError>,
}

impl FakePlatform {
    fn ready(declarations: Vec<DataSourceDeclaration>, placements: Vec<PlacementRecord>) -> Self {
        Self {
            data_sources: Ok(DataSourcesRead {
                revision: Some(DesiredRevision::new("ds-1")),
                declarations,
            }),
            placements: Ok(PlacementsRead {
                revision: Some(DesiredRevision::new("pl-1")),
                placements,
            }),
        }
    }

    fn data_sources_unavailable() -> Self {
        Self {
            data_sources: Err(DesiredStateError::Unavailable {
                detail: "the platform repository could not be reached".to_owned(),
            }),
            placements: Ok(PlacementsRead {
                revision: Some(DesiredRevision::new("pl-1")),
                placements: vec![],
            }),
        }
    }
}

#[async_trait::async_trait]
impl DataSourceState for FakePlatform {
    async fn read_data_sources(&self, _environment: &str) -> Result<DataSourcesRead, DesiredStateError> {
        self.data_sources.clone()
    }

    async fn write_data_sources(
        &self,
        _: &str,
        _: &[DataSourceDeclaration],
        _: Option<&DesiredRevision>,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a publication pass never writes data sources")
    }
}

#[async_trait::async_trait]
impl PlacementState for FakePlatform {
    async fn read_placements(&self, _environment: &str) -> Result<PlacementsRead, DesiredStateError> {
        self.placements.clone()
    }

    async fn write_placements(
        &self,
        _: &str,
        _: &[PlacementRecord],
        _: Option<&DesiredRevision>,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a publication pass never writes placements")
    }
}

#[async_trait::async_trait]
impl DesiredState for FakePlatform {
    async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
        panic!("a publication pass never sweeps components")
    }

    async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
        panic!("a publication pass never reads a component")
    }

    async fn advance(
        &self,
        _: &str,
        _: &str,
        _: &Release,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a publication pass never advances a component")
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        _: &Release,
        _: &Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a publication pass never rolls back a component")
    }

    async fn pause(
        &self,
        _: &str,
        _: &str,
        _: &Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a publication pass never pauses a component")
    }

    async fn resume(&self, _: &str, _: &str, _: &DesiredRevision, _: &str) -> Result<(), DesiredStateError> {
        panic!("a publication pass never resumes a component")
    }
}

#[async_trait::async_trait]
impl PlatformRepository for FakePlatform {
    async fn write_environment(
        &self,
        _: &str,
        _: EnvironmentWrite<'_>,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a publication pass never writes the environment")
    }
}

/// A [`RuntimeCatalogueSource`] that answers once, from what it was built
/// with.
struct FakeCatalogue(Result<CatalogDocument, CatalogueSourceError>);

#[async_trait::async_trait]
impl RuntimeCatalogueSource for FakeCatalogue {
    async fn runtime_catalogue(&self) -> Result<CatalogDocument, CatalogueSourceError> {
        self.0.clone()
    }
}

/// [`FakeCatalogue`], but notifies `entered` and waits on `release` first --
/// long enough for a concurrent second pass to observe the guard held.
struct GatedCatalogue {
    inner: FakeCatalogue,
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait::async_trait]
impl RuntimeCatalogueSource for GatedCatalogue {
    async fn runtime_catalogue(&self) -> Result<CatalogDocument, CatalogueSourceError> {
        self.entered.notify_one();
        self.release.notified().await;
        self.inner.runtime_catalogue().await
    }
}

fn publisher(
    platform: Arc<dyn PlatformRepository>,
    catalogue: Arc<dyn RuntimeCatalogueSource>,
    target: Arc<dyn fabric_runtime_publication::RuntimePublication>,
) -> RuntimePublisher {
    RuntimePublisher::new(
        "lucentroot".to_owned(),
        platform,
        catalogue,
        target,
        Arc::new(SystemClock::new()) as Arc<dyn Clock>,
    )
}

#[tokio::test]
async fn waiting_when_the_catalogue_has_no_resources() {
    let publisher = publisher(
        Arc::new(FakePlatform::ready(vec![], vec![])),
        Arc::new(FakeCatalogue(Ok(CatalogDocument::new(BTreeMap::new())))),
        Arc::new(FakePublication::new()),
    );

    let result = publisher.publish_once(&PublicationState::new()).await;

    assert!(
        matches!(
            result,
            PassResult::Ran(PassOutcome::Waiting {
                reason: WaitingReason::NoResources
            })
        ),
        "{result:?}"
    );
}

#[tokio::test]
async fn refused_when_the_held_data_sources_are_incoherent() {
    let publisher = publisher(
        Arc::new(FakePlatform::ready(
            vec![declaration("dup"), declaration("dup")],
            vec![],
        )),
        Arc::new(FakeCatalogue(Ok(catalog_with_one_resource()))),
        Arc::new(FakePublication::new()),
    );

    let result = publisher.publish_once(&PublicationState::new()).await;

    assert!(
        matches!(result, PassResult::Ran(PassOutcome::Refused { .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn refused_on_a_catalogue_conflict() {
    let publisher = publisher(
        Arc::new(FakePlatform::ready(vec![], vec![])),
        Arc::new(FakeCatalogue(Err(CatalogueSourceError::Conflict {
            resource: "customers".to_owned(),
            applications: ("crm".to_owned(), "billing".to_owned()),
        }))),
        Arc::new(FakePublication::new()),
    );

    let result = publisher.publish_once(&PublicationState::new()).await;

    assert!(
        matches!(result, PassResult::Ran(PassOutcome::Refused { .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn failed_when_an_input_cannot_be_reached() {
    let publisher = publisher(
        Arc::new(FakePlatform::data_sources_unavailable()),
        Arc::new(FakeCatalogue(Ok(catalog_with_one_resource()))),
        Arc::new(FakePublication::new()),
    );
    let state = PublicationState::new();

    let first = publisher.publish_once(&state).await;
    assert!(
        matches!(first, PassResult::Ran(PassOutcome::Failed { .. })),
        "{first:?}"
    );

    // The guard is released after a failing pass: a second call runs again
    // rather than answering `AlreadyRunning`.
    let second = publisher.publish_once(&state).await;
    assert!(
        matches!(second, PassResult::Ran(PassOutcome::Failed { .. })),
        "{second:?}"
    );
}

#[tokio::test]
async fn published_then_unchanged_on_an_identical_republish() {
    let publisher = publisher(
        Arc::new(FakePlatform::ready(
            vec![declaration("shared-a")],
            vec![placement("acme", "shared-a")],
        )),
        Arc::new(FakeCatalogue(Ok(catalog_with_one_resource()))),
        Arc::new(FakePublication::new()),
    );
    let state = PublicationState::new();

    let first = publisher.publish_once(&state).await;
    assert!(
        matches!(first, PassResult::Ran(PassOutcome::Published { .. })),
        "{first:?}"
    );

    let second = publisher.publish_once(&state).await;
    assert!(
        matches!(second, PassResult::Ran(PassOutcome::Unchanged { .. })),
        "{second:?}"
    );
}

#[tokio::test]
async fn a_pass_already_running_is_skipped_rather_than_queued() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let catalogue = Arc::new(GatedCatalogue {
        inner: FakeCatalogue(Ok(catalog_with_one_resource())),
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
    });

    let gated = Arc::new(publisher(
        Arc::new(FakePlatform::ready(
            vec![declaration("shared-a")],
            vec![placement("acme", "shared-a")],
        )),
        catalogue,
        Arc::new(FakePublication::new()),
    ));
    let state = Arc::new(PublicationState::new());

    let running = tokio::spawn({
        let gated = Arc::clone(&gated);
        let state = Arc::clone(&state);
        async move { gated.publish_once(&state).await }
    });

    entered.notified().await;

    let attempted = gated.publish_once(&state).await;
    assert!(matches!(attempted, PassResult::AlreadyRunning), "{attempted:?}");

    release.notify_one();
    let finished = running.await.expect("the spawned pass completes");
    assert!(matches!(finished, PassResult::Ran(_)), "{finished:?}");

    // And once it has finished, the guard is free again -- checked against a
    // second, ungated publisher over the same state, so this assertion does
    // not itself need to feed the first publisher's gate a second time.
    let ungated = publisher(
        Arc::new(FakePlatform::ready(
            vec![declaration("shared-a")],
            vec![placement("acme", "shared-a")],
        )),
        Arc::new(FakeCatalogue(Ok(catalog_with_one_resource()))),
        Arc::new(FakePublication::new()),
    );
    assert!(matches!(ungated.publish_once(&state).await, PassResult::Ran(_)));
}

/// A [`RuntimePublication`] whose `current` always panics -- proving the
/// guard is released by `RunningGuard`'s `Drop`, not by code sequenced after
/// an `.await` that a panic unwinding through it never reaches.
struct PanickingPublication;

#[async_trait::async_trait]
impl RuntimePublication for PanickingPublication {
    async fn current(&self) -> Result<PublishedRevisions, PublicationError> {
        panic!("simulated target failure")
    }

    async fn publish(&self, _: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
        unreachable!("current() always panics first")
    }

    fn describe(&self) -> String {
        "panicking".to_owned()
    }
}

#[tokio::test]
async fn the_guard_releases_even_when_the_target_panics() {
    let state = Arc::new(PublicationState::new());

    // Run inside a spawned task so the panic is caught as a `JoinError`
    // rather than aborting the test process -- what actually matters is
    // what it leaves behind in `state`, not how the panic itself surfaces.
    let panicked = tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            let panicking = publisher(
                Arc::new(FakePlatform::ready(vec![], vec![])),
                Arc::new(FakeCatalogue(Ok(catalog_with_one_resource()))),
                Arc::new(PanickingPublication),
            );
            panicking.publish_once(&state).await
        }
    })
    .await;
    assert!(panicked.is_err(), "current() was made to panic");

    // Proved against a second, independently built publisher over the same
    // `state` -- the panicking one above is useless for a second call, since
    // its target panics unconditionally, but the guard it held is
    // `PublicationState`'s, and this is the same state.
    let sane = publisher(
        Arc::new(FakePlatform::ready(vec![], vec![])),
        Arc::new(FakeCatalogue(Ok(catalog_with_one_resource()))),
        Arc::new(FakePublication::new()),
    );
    assert!(
        matches!(sane.publish_once(&state).await, PassResult::Ran(_)),
        "the guard must be released after a panic, not left AlreadyRunning forever"
    );
}

#[tokio::test]
async fn the_guard_releases_when_the_pass_is_cancelled_mid_flight() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let catalogue = Arc::new(GatedCatalogue {
        inner: FakeCatalogue(Ok(catalog_with_one_resource())),
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
    });

    let gated = publisher(
        Arc::new(FakePlatform::ready(vec![], vec![])),
        catalogue,
        Arc::new(FakePublication::new()),
    );
    let state = PublicationState::new();

    // `select!` polls both branches on this same task -- no second task, no
    // `abort`, nothing that depends on how a runtime schedules cancellation.
    // Once `entered` resolves, `publish_once(&state)` is mid-read, parked
    // inside `GatedCatalogue::runtime_catalogue`'s own wait on `release`;
    // `select!` drops that losing branch right there, which is exactly the
    // cancellation a dropped handler future (an operator's disconnect, a
    // request timeout) produces -- `release` is never notified, so the pass
    // could not have finished on its own.
    tokio::select! {
        _ = gated.publish_once(&state) => {
            panic!("the pass must not finish: release is never notified in this test");
        }
        () = entered.notified() => {}
    }

    // Proved against a second, independently built publisher: `gated`
    // above still holds the same `GatedCatalogue`, which would block again
    // on `release` -- nobody ever notifies -- so reusing it here would
    // hang the very call meant to prove the guard is free, not the
    // cancellation this test exists to check.
    let sane = publisher(
        Arc::new(FakePlatform::ready(vec![], vec![])),
        Arc::new(FakeCatalogue(Ok(catalog_with_one_resource()))),
        Arc::new(FakePublication::new()),
    );
    assert!(
        matches!(sane.publish_once(&state).await, PassResult::Ran(_)),
        "the guard must be released after cancellation, not left AlreadyRunning forever"
    );
}
