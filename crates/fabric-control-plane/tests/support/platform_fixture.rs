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
    DesiredRevision, DesiredState, DesiredStateError, Hold, PlatformDesiredState, PlatformManagement,
    PlatformRepository, Registry, RegistryError, Release, Resolved, Version,
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
}

impl FakeRepository {
    /// Starts with nothing declared.
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Held {
                revision: None,
                declarations: Vec::new(),
                writes: 0,
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

    /// Sets what is held directly, bypassing `DataSources::declare`'s own
    /// validation -- the shape a hand edit on disk can reach (ADR 0023 part
    /// 1's break-glass path), which `check_held` exists to catch on the
    /// next read rather than trust.
    pub fn seed(&self, revision: DesiredRevision, declarations: Vec<DataSourceDeclaration>) {
        let mut held = self.inner.lock().expect("the fixture lock is never poisoned");

        held.revision = Some(revision);
        held.declarations = declarations;
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
        repository,
    };

    (binding, fake)
}
