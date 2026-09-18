//! A `PlatformRepository` fake, connected through the real
//! `PlatformDesiredState` binding, for tests that drive
//! `/api/platform/data-sources` through the real router.
//!
//! Built the same way `fabric-control-plane-api`'s `startup::platform::establish`
//! builds the shipping composition: one `PlatformDesiredState`, connected once,
//! with `PlatformManagement` and `DataSources` both built over it -- so a test
//! exercises the generation tagging and the always-present revision the real
//! binding gives, not a shortcut that reaches the fake directly.

use std::sync::{Arc, Mutex};

use fabric_control_plane::PlatformBinding;
use fabric_core::Clock;
use fabric_platform_management::{
    ChartIndex, ComponentDesired, DataSourceDeclaration, DataSourceState, DataSources, DataSourcesRead,
    DesiredRevision, DesiredState, DesiredStateError, EnvironmentWrite, Hold, PlacementRecord,
    PlacementState, Placements, PlacementsRead, PlatformDesiredState, PlatformManagement, PlatformRepository,
    Registry, RegistryError, Release, Resolved, Version,
};

use super::FixedClock;

/// The environment every fixture built here manages.
pub const ENVIRONMENT: &str = "lucentroot";

/// A `PlatformRepository`: `DataSourceState` backed by an in-memory store
/// with the same compare-and-swap contract the real adapter gives, and
/// `DesiredState` answered trivially -- nothing under test here reads or
/// moves a component, only data sources.
pub struct FakeRepository {
    inner: Mutex<Held>,
}

/// What is currently declared, and how many times a write actually landed.
struct Held {
    revision: Option<DesiredRevision>,
    declarations: Vec<DataSourceDeclaration>,
    writes: u32,

    /// The placements half (ADR 0023 part 2), generation-free the same way
    /// the declarations above are: this fixture is connected through the
    /// real `PlatformDesiredState` binding, which is what actually tags
    /// every revision it hands out -- what this struct holds underneath
    /// that is a bare `Option`, exactly like `revision` above.
    placements_revision: Option<DesiredRevision>,
    placements: Vec<PlacementRecord>,
    placement_writes: u32,
}

impl FakeRepository {
    /// Starts with nothing declared and nothing placed.
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Held {
                revision: None,
                declarations: Vec::new(),
                writes: 0,
                placements_revision: None,
                placements: Vec::new(),
                placement_writes: 0,
            }),
        })
    }

    /// How many times `write_data_sources` actually wrote, as opposed to
    /// being asked to and finding nothing had changed.
    pub fn writes(&self) -> u32 {
        self.inner
            .lock()
            .expect("the fixture lock is never poisoned")
            .writes
    }

    /// [`Self::writes`]'s sibling for `write_placements`.
    pub fn placement_writes(&self) -> u32 {
        self.inner
            .lock()
            .expect("the fixture lock is never poisoned")
            .placement_writes
    }

    /// Sets what is held directly, bypassing `DataSources::declare`'s own
    /// validation -- the shape a hand edit on disk can reach (ADR 0023 part
    /// 1's break-glass path), which `check_held` exists to catch on the
    /// next read rather than trust.
    pub fn seed(&self, revision: DesiredRevision, declarations: Vec<DataSourceDeclaration>) {
        let mut held = self.inner.lock().expect("the fixture lock is never poisoned");

        held.revision = Some(revision);
        held.declarations = declarations;
    }

    /// [`Self::seed`]'s sibling for the placements half (ADR 0023 part 2):
    /// sets what is recorded directly, bypassing `Placements::place`'s own
    /// selection, the shape a break-glass hand edit to `placements.yaml`
    /// can reach -- which `check_held_placements` exists to catch on the
    /// next read rather than trust.
    pub fn seed_placements(&self, revision: DesiredRevision, placements: Vec<PlacementRecord>) {
        let mut held = self.inner.lock().expect("the fixture lock is never poisoned");

        held.placements_revision = Some(revision);
        held.placements = placements;
    }
}

#[async_trait::async_trait]
impl DataSourceState for FakeRepository {
    async fn read_data_sources(&self, _environment: &str) -> Result<DataSourcesRead, DesiredStateError> {
        let held = self.inner.lock().expect("the fixture lock is never poisoned");

        Ok(DataSourcesRead {
            revision: held.revision.clone(),
            declarations: held.declarations.clone(),
        })
    }

    async fn write_data_sources(
        &self,
        _environment: &str,
        declarations: &[DataSourceDeclaration],
        at: Option<&DesiredRevision>,
        _message: &str,
    ) -> Result<(), DesiredStateError> {
        let mut held = self.inner.lock().expect("the fixture lock is never poisoned");

        if held.revision.as_ref() != at {
            return Err(DesiredStateError::Conflict);
        }

        held.writes += 1;
        held.revision = Some(DesiredRevision::new(format!("rev-{}", held.writes)));
        held.declarations = declarations.to_vec();
        Ok(())
    }
}

#[async_trait::async_trait]
impl PlacementState for FakeRepository {
    async fn read_placements(&self, _environment: &str) -> Result<PlacementsRead, DesiredStateError> {
        let held = self.inner.lock().expect("the fixture lock is never poisoned");

        Ok(PlacementsRead {
            revision: held.placements_revision.clone(),
            placements: held.placements.clone(),
        })
    }

    async fn write_placements(
        &self,
        _environment: &str,
        placements: &[PlacementRecord],
        at: Option<&DesiredRevision>,
        _message: &str,
    ) -> Result<(), DesiredStateError> {
        let mut held = self.inner.lock().expect("the fixture lock is never poisoned");

        if held.placements_revision.as_ref() != at {
            return Err(DesiredStateError::Conflict);
        }

        held.placement_writes += 1;
        held.placements_revision = Some(DesiredRevision::new(format!(
            "placements-rev-{}",
            held.placement_writes
        )));
        held.placements = placements.to_vec();
        Ok(())
    }
}

/// The two-document write ADR 0023 part 2 (B4) requires: both halves of
/// `Held` are behind the one `Mutex`, so checking both revisions and
/// applying both writes under the one lock is already the atomicity
/// `write_environment` promises -- there is no second lock to coordinate.
///
/// Each half's revision moves only when that half's *content* actually
/// changed, mirroring the real adapter: `PlatformGitRepository` writes
/// content-addressed blobs, so re-rendering a document nothing changed
/// produces the same bytes and the same hash. A fixture that bumped an
/// unconditional counter on every call would move an operator's
/// data-sources `ETag` on every placement, which the real repository does
/// not do and no test here should have to work around.
#[async_trait::async_trait]
impl PlatformRepository for FakeRepository {
    async fn write_environment(
        &self,
        _environment: &str,
        write: EnvironmentWrite<'_>,
        _message: &str,
    ) -> Result<(), DesiredStateError> {
        let mut held = self.inner.lock().expect("the fixture lock is never poisoned");

        if held.revision.as_ref() != write.data_sources.1 {
            return Err(DesiredStateError::Conflict);
        }
        if held.placements_revision.as_ref() != write.placements.1 {
            return Err(DesiredStateError::Conflict);
        }

        if held.declarations != write.data_sources.0 {
            held.writes += 1;
            held.revision = Some(DesiredRevision::new(format!("rev-{}", held.writes)));
            held.declarations = write.data_sources.0.to_vec();
        }

        if held.placements != write.placements.0 {
            held.placement_writes += 1;
            held.placements_revision = Some(DesiredRevision::new(format!(
                "placements-rev-{}",
                held.placement_writes
            )));
            held.placements = write.placements.0.to_vec();
        }

        Ok(())
    }
}

/// `DesiredState` no test built over this fixture calls -- nothing here
/// drives GET /api/platform or anything that reads or moves a component.
/// Answered trivially so `FakeRepository` satisfies `PlatformRepository`,
/// the one thing `PlatformDesiredState::connect` accepts.
#[async_trait::async_trait]
impl DesiredState for FakeRepository {
    async fn components(&self, _environment: &str) -> Result<Vec<String>, DesiredStateError> {
        Ok(Vec::new())
    }

    async fn component(
        &self,
        _environment: &str,
        component: &str,
    ) -> Result<ComponentDesired, DesiredStateError> {
        Err(DesiredStateError::NotFound {
            what: component.to_owned(),
        })
    }

    async fn advance(
        &self,
        _environment: &str,
        _component: &str,
        _release: &Release,
        _at: &DesiredRevision,
        _message: &str,
    ) -> Result<(), DesiredStateError> {
        Err(DesiredStateError::NotConnected)
    }

    async fn roll_back(
        &self,
        _environment: &str,
        _component: &str,
        _release: &Release,
        _hold: &Hold,
        _at: &DesiredRevision,
        _message: &str,
    ) -> Result<(), DesiredStateError> {
        Err(DesiredStateError::NotConnected)
    }

    async fn pause(
        &self,
        _environment: &str,
        _component: &str,
        _hold: &Hold,
        _at: &DesiredRevision,
        _message: &str,
    ) -> Result<(), DesiredStateError> {
        Err(DesiredStateError::NotConnected)
    }

    async fn resume(
        &self,
        _environment: &str,
        _component: &str,
        _at: &DesiredRevision,
        _message: &str,
    ) -> Result<(), DesiredStateError> {
        Err(DesiredStateError::NotConnected)
    }
}

/// A registry no test built over this fixture asks -- nothing here drives
/// GET /api/platform or anything that discovers a version.
struct UnusedRegistry;

#[async_trait::async_trait]
impl Registry for UnusedRegistry {
    async fn tags(&self, _repository: &str) -> Result<Vec<String>, RegistryError> {
        Ok(Vec::new())
    }

    async fn resolve(&self, _repository: &str, _tag: &str) -> Result<Option<Resolved>, RegistryError> {
        Ok(None)
    }
}

/// `UnusedRegistry`'s sibling for chart versions.
struct UnusedChartIndex;

#[async_trait::async_trait]
impl ChartIndex for UnusedChartIndex {
    async fn versions(&self, _repository: &str, _chart: &str) -> Result<Vec<Version>, RegistryError> {
        Ok(Vec::new())
    }
}

/// Builds a `PlatformBinding` the way `fabric-control-plane-api` builds the
/// shipping one -- one `PlatformDesiredState`, connected to the in-memory
/// fake, with `PlatformManagement` and `DataSources` both built over it --
/// and hands back the fake itself so a test can read what it recorded.
pub async fn platform_binding() -> (PlatformBinding, Arc<FakeRepository>) {
    let fake = FakeRepository::new();
    let repository = PlatformDesiredState::unconnected();
    repository
        .connect(Arc::clone(&fake) as Arc<dyn PlatformRepository>)
        .await;

    let service = PlatformManagement::new(
        Arc::new(UnusedRegistry) as Arc<dyn Registry>,
        Arc::new(UnusedChartIndex) as Arc<dyn ChartIndex>,
        Arc::clone(&repository) as Arc<dyn DesiredState>,
        Arc::new(FixedClock) as Arc<dyn Clock>,
    );

    let binding = PlatformBinding {
        service: Arc::new(service),
        environment: ENVIRONMENT.to_owned(),
        data_sources: Arc::new(DataSources::new(
            Arc::clone(&repository) as Arc<dyn DataSourceState>
        )),
        placements: Arc::new(Placements::new(
            Arc::clone(&repository) as Arc<dyn PlatformRepository>,
            Arc::new(FixedClock) as Arc<dyn Clock>,
        )),
        repository,
    };

    (binding, fake)
}
