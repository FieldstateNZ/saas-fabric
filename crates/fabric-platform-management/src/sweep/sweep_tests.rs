//! A sweep looks after every component, and keeps going when one cannot be.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::{
    ArtifactSource, Channel, ChartIndex, CheckOutcome, ComponentDesired, DesiredState, DesiredStateError,
    PlatformManagement, Provenance, Registry, RegistryError, Release, ReleaseUnit, Resolved, SweepResult,
    SweepState, Swept, UpdatePolicy, Version,
};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";

fn version(text: &str) -> Version {
    Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
}

/// A registry with `preview.3` complete for every component.
struct Registries;

#[async_trait::async_trait]
impl Registry for Registries {
    async fn tags(&self, _: &str) -> Result<Vec<String>, RegistryError> {
        Ok(vec!["0.3.0-preview.2".to_owned(), "0.3.0-preview.3".to_owned()])
    }

    async fn resolve(&self, _: &str, tag: &str) -> Result<Option<Resolved>, RegistryError> {
        Ok(Some(Resolved {
            digest: format!("sha256:{}", tag.len()),
            provenance: Provenance::Agreed("bbbb".to_owned()),
        }))
    }
}

/// Several components, one of which cannot be read.
struct Several {
    /// Component name to its desired state, or the failure reading it.
    components: Mutex<BTreeMap<String, Result<ComponentDesired, DesiredStateError>>>,

    /// Components that were advanced, in order.
    advanced: Mutex<Vec<String>>,
}

impl Several {
    fn new() -> Self {
        let describe = |policy| {
            Ok(ComponentDesired {
                version: version("0.3.0-preview.2"),
                channel: Channel::Preview,
                policy,
                hold: None,
                source: ArtifactSource::Oci {
                    repositories: BTreeMap::from([("runtime".to_owned(), RUNTIME.to_owned())]),
                },
            })
        };

        Self {
            components: Mutex::new(BTreeMap::from([
                (
                    "a-broken".to_owned(),
                    Err(DesiredStateError::Unavailable {
                        detail: "the store said no".to_owned(),
                    }),
                ),
                ("b-automatic".to_owned(), describe(UpdatePolicy::Automatic)),
                ("c-manual".to_owned(), describe(UpdatePolicy::Manual)),
            ])),
            advanced: Mutex::new(Vec::new()),
        }
    }

    fn advanced(&self) -> Vec<String> {
        self.advanced
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl DesiredState for Several {
    async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
        Ok(self
            .components
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect())
    }

    async fn component(&self, _: &str, component: &str) -> Result<ComponentDesired, DesiredStateError> {
        self.components
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(component)
            .cloned()
            .unwrap_or(Err(DesiredStateError::NotFound {
                what: component.to_owned(),
            }))
    }

    async fn advance(
        &self,
        _: &str,
        component: &str,
        release: &Release,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        self.advanced
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(component.to_owned());

        if let Ok(desired) = self
            .components
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(component)
            .expect("a component that was advanced exists")
        {
            desired.version = release.version().clone();
        }

        Ok(())
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        _: &ReleaseUnit,
        _: &crate::Hold,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a sweep must never roll a component back")
    }

    async fn pause(&self, _: &str, _: &str, _: &crate::Hold, _: &str) -> Result<(), DesiredStateError> {
        panic!("a sweep must never pause a component")
    }

    async fn resume(&self, _: &str, _: &str, _: &str) -> Result<(), DesiredStateError> {
        panic!("a sweep must never resume a component")
    }
}

fn service(desired_state: &Arc<Several>) -> PlatformManagement {
    PlatformManagement::new(
        Arc::new(Registries) as Arc<dyn Registry>,
        Arc::new(Charts::default()) as Arc<dyn ChartIndex>,
        Arc::clone(desired_state) as Arc<dyn DesiredState>,
        Arc::new(fabric_core::SystemClock::new()) as Arc<dyn fabric_core::Clock>,
    )
}

/// A chart repository holding stated versions.
#[derive(Default)]
struct Charts {
    /// Versions by (repository, chart).
    published: Mutex<BTreeMap<(String, String), Vec<Version>>>,
}

#[async_trait::async_trait]
impl ChartIndex for Charts {
    async fn versions(&self, repository: &str, chart: &str) -> Result<Vec<Version>, RegistryError> {
        Ok(self
            .published
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(repository.to_owned(), chart.to_owned()))
            .cloned()
            .unwrap_or_default())
    }
}

#[tokio::test]
async fn one_component_failing_does_not_stop_the_others() {
    // `a-broken` sorts first, so a sweep that gave up on the first failure
    // would leave every component after it permanently unreconciled -- in an
    // order nobody chose.
    let desired_state = Arc::new(Several::new());
    let service = service(&desired_state);

    let SweepResult::Ran(sweep) = service.sweep("lucentroot", &SweepState::default()).await.unwrap() else {
        panic!("nothing else was running");
    };

    assert!(sweep.had_failures());
    assert_eq!(sweep.components.len(), 3);
    assert!(matches!(sweep.components[0], (ref name, Swept::Failed(_)) if name == "a-broken"));
    assert_eq!(desired_state.advanced(), vec!["b-automatic".to_owned()]);
}

#[tokio::test]
async fn only_the_automatic_component_moves() {
    let desired_state = Arc::new(Several::new());
    let service = service(&desired_state);

    let SweepResult::Ran(sweep) = service.sweep("lucentroot", &SweepState::default()).await.unwrap() else {
        panic!("nothing else was running");
    };

    let advanced: Vec<&String> = sweep
        .components
        .iter()
        .filter(|(_, swept)| matches!(swept, Swept::Advanced { .. }))
        .map(|(name, _)| name)
        .collect();

    assert_eq!(advanced, vec!["b-automatic"]);
    assert!(
        matches!(sweep.components[2], (ref name, Swept::Unchanged) if name == "c-manual"),
        "a manual component was touched"
    );
}

#[tokio::test]
async fn an_advance_says_where_it_came_from() {
    let desired_state = Arc::new(Several::new());
    let service = service(&desired_state);

    let SweepResult::Ran(sweep) = service.sweep("lucentroot", &SweepState::default()).await.unwrap() else {
        panic!("nothing else was running");
    };

    let (_, swept) = &sweep.components[1];
    assert_eq!(
        swept,
        &Swept::Advanced {
            from: version("0.3.0-preview.2"),
            to: version("0.3.0-preview.3"),
        }
    );
}

#[tokio::test]
async fn a_second_sweep_finds_nothing_left_to_do() {
    let desired_state = Arc::new(Several::new());
    let service = service(&desired_state);
    let guard = SweepState::default();

    service.sweep("lucentroot", &guard).await.unwrap();
    let SweepResult::Ran(second) = service.sweep("lucentroot", &guard).await.unwrap() else {
        panic!("the first sweep finished");
    };

    assert!(
        !second
            .components
            .iter()
            .any(|(_, swept)| matches!(swept, Swept::Advanced { .. })),
        "a settled environment advanced again"
    );
    assert_eq!(
        desired_state.advanced().len(),
        1,
        "the same version was written twice"
    );
}

#[tokio::test]
async fn nothing_having_checked_yet_is_its_own_answer() {
    // Three explanations for a version not appearing, and they lead three
    // different places: nothing has checked, something checked and found
    // nothing to do, or something checked and failed. `None` is the first.
    let state = SweepState::default();

    assert_eq!(state.last_check(), None);
}

#[tokio::test]
async fn a_sweep_records_what_it_found() {
    let desired_state = Arc::new(Several::new());
    let service = service(&desired_state);
    let state = SweepState::default();

    service.sweep("lucentroot", &state).await.unwrap();

    let check = state.last_check().expect("a sweep ran");
    assert!(check.at_unix_seconds > 0);

    // `a-broken` cannot be read, so the sweep did not wholly succeed -- and
    // saying so is the difference between "found nothing" and "could not look".
    let CheckOutcome::Failed { detail } = check.outcome else {
        panic!("a sweep with a broken component reported success");
    };
    assert!(detail.as_str().starts_with("a-broken:"), "{detail}");
}

#[tokio::test]
async fn a_sweep_with_nothing_wrong_records_success() {
    let desired_state = Arc::new(Several::new());
    desired_state
        .components
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove("a-broken");

    let service = service(&desired_state);
    let state = SweepState::default();

    service.sweep("lucentroot", &state).await.unwrap();

    assert_eq!(
        state.last_check().expect("a sweep ran").outcome,
        CheckOutcome::Succeeded
    );
}

/// Desired state that parks inside the first read until it is released.
struct Gated {
    inner: Several,
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    first: Mutex<bool>,
}

#[async_trait::async_trait]
impl DesiredState for Gated {
    async fn components(&self, environment: &str) -> Result<Vec<String>, DesiredStateError> {
        self.inner.components(environment).await
    }

    async fn component(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<ComponentDesired, DesiredStateError> {
        let park = {
            let mut first = self.first.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::replace(&mut *first, false)
        };

        if park {
            self.entered.notify_one();
            self.release.notified().await;
        }

        self.inner.component(environment, component).await
    }

    async fn advance(&self, e: &str, c: &str, release: &Release, m: &str) -> Result<(), DesiredStateError> {
        self.inner.advance(e, c, release, m).await
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        _: &ReleaseUnit,
        _: &crate::Hold,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a sweep must never roll a component back")
    }

    async fn pause(&self, _: &str, _: &str, _: &crate::Hold, _: &str) -> Result<(), DesiredStateError> {
        panic!("a sweep must never pause a component")
    }

    async fn resume(&self, _: &str, _: &str, _: &str) -> Result<(), DesiredStateError> {
        panic!("a sweep must never resume a component")
    }
}

#[tokio::test]
async fn a_sweep_already_running_is_skipped_rather_than_queued() {
    // A sweep that overruns its interval means registries or Git are slow, and
    // the answer to that is not to start a second one behind it.
    //
    // The first sweep is genuinely parked mid-flight when the second is
    // attempted. Calling them one after the other would prove nothing: the
    // first would have finished and released the guard before the second
    // looked at it.
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let desired_state = Arc::new(Gated {
        inner: Several::new(),
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
        first: Mutex::new(true),
    });

    let service = Arc::new(PlatformManagement::new(
        Arc::new(Registries) as Arc<dyn Registry>,
        Arc::new(Charts::default()) as Arc<dyn ChartIndex>,
        Arc::clone(&desired_state) as Arc<dyn DesiredState>,
        Arc::new(fabric_core::SystemClock::new()) as Arc<dyn fabric_core::Clock>,
    ));
    let guard = Arc::new(SweepState::default());

    let running = tokio::spawn({
        let service = Arc::clone(&service);
        let guard = Arc::clone(&guard);
        async move { service.sweep("lucentroot", &guard).await }
    });

    entered.notified().await;

    let attempted = service.sweep("lucentroot", &guard).await.unwrap();
    assert_eq!(attempted, SweepResult::AlreadyRunning);

    release.notify_one();
    assert!(matches!(running.await.unwrap().unwrap(), SweepResult::Ran(_)));

    // And once it has finished, the guard is free again.
    assert!(matches!(
        service.sweep("lucentroot", &guard).await.unwrap(),
        SweepResult::Ran(_)
    ));
}

#[tokio::test]
async fn a_sweep_with_nothing_connected_records_nothing() {
    // An operator has not connected a platform repository. That is a state, not
    // a failure -- and a "last check failed" against an integration nobody has
    // made would send them looking for a fault instead of a connection.
    struct Unconnected;

    #[async_trait::async_trait]
    impl DesiredState for Unconnected {
        async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
            Err(DesiredStateError::NotConnected)
        }

        async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
            Err(DesiredStateError::NotConnected)
        }

        async fn advance(&self, _: &str, _: &str, _: &Release, _: &str) -> Result<(), DesiredStateError> {
            Err(DesiredStateError::NotConnected)
        }

        async fn roll_back(
            &self,
            _: &str,
            _: &str,
            _: &ReleaseUnit,
            _: &crate::Hold,
            _: &str,
        ) -> Result<(), DesiredStateError> {
            Err(DesiredStateError::NotConnected)
        }

        async fn pause(&self, _: &str, _: &str, _: &crate::Hold, _: &str) -> Result<(), DesiredStateError> {
            Err(DesiredStateError::NotConnected)
        }

        async fn resume(&self, _: &str, _: &str, _: &str) -> Result<(), DesiredStateError> {
            Err(DesiredStateError::NotConnected)
        }
    }

    let service = PlatformManagement::new(
        Arc::new(Registries) as Arc<dyn Registry>,
        Arc::new(Charts::default()) as Arc<dyn ChartIndex>,
        Arc::new(Unconnected) as Arc<dyn DesiredState>,
        Arc::new(fabric_core::SystemClock::new()) as Arc<dyn fabric_core::Clock>,
    );
    let state = SweepState::default();

    assert_eq!(
        service.sweep("lucentroot", &state).await.unwrap(),
        SweepResult::NotConnected
    );
    assert_eq!(
        state.last_check(),
        None,
        "nothing looked, so nothing was recorded"
    );
}
