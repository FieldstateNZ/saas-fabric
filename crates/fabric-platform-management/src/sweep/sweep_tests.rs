//! A sweep looks after every component, and keeps going when one cannot be.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::{
    ArtifactSource, Channel, ChartIndex, CheckOutcome, ComponentDesired, DesiredRevision, DesiredState,
    DesiredStateError, PlatformManagement, Provenance, Registry, RegistryError, Release, Resolved,
    SweepResult, SweepState, Swept, UpdatePolicy, Version,
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
                revision: DesiredRevision::new("read-1"),
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
        _: &DesiredRevision,
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
        _: &Release,
        _: &crate::Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a sweep must never roll a component back")
    }

    async fn pause(
        &self,
        _: &str,
        _: &str,
        _: &crate::Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a sweep must never pause a component")
    }

    async fn resume(&self, _: &str, _: &str, _: &DesiredRevision, _: &str) -> Result<(), DesiredStateError> {
        panic!("a sweep must never resume a component")
    }
}

/// Builds a service over any `DesiredState`, not only `Several` -- the
/// fixtures that gate or panic mid-read wrap something other than
/// `Several`, so they cannot go through `service` below.
fn service_over(desired_state: Arc<dyn DesiredState>) -> PlatformManagement {
    PlatformManagement::new(
        Arc::new(Registries) as Arc<dyn Registry>,
        Arc::new(Charts::default()) as Arc<dyn ChartIndex>,
        desired_state,
        Arc::new(fabric_core::SystemClock::new()) as Arc<dyn fabric_core::Clock>,
    )
}

fn service(desired_state: &Arc<Several>) -> PlatformManagement {
    service_over(Arc::clone(desired_state) as Arc<dyn DesiredState>)
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

    async fn advance(
        &self,
        e: &str,
        c: &str,
        release: &Release,
        at: &DesiredRevision,
        m: &str,
    ) -> Result<(), DesiredStateError> {
        self.inner.advance(e, c, release, at, m).await
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        _: &Release,
        _: &crate::Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a sweep must never roll a component back")
    }

    async fn pause(
        &self,
        _: &str,
        _: &str,
        _: &crate::Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        panic!("a sweep must never pause a component")
    }

    async fn resume(&self, _: &str, _: &str, _: &DesiredRevision, _: &str) -> Result<(), DesiredStateError> {
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

    let service = Arc::new(service_over(Arc::clone(&desired_state) as Arc<dyn DesiredState>));
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
async fn the_guard_releases_when_the_sweep_is_cancelled_mid_flight() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let desired_state = Arc::new(Gated {
        inner: Several::new(),
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
        first: Mutex::new(true),
    });

    let gated = service_over(desired_state as Arc<dyn DesiredState>);
    let state = SweepState::default();

    // `select!` polls both branches on this same task -- no second task, no
    // `abort`, nothing that depends on how a runtime schedules cancellation.
    // Once `entered` resolves, `gated.sweep(env, &state)` is mid-read, parked
    // inside `Gated::component`'s own wait on `release`; `select!` drops
    // that losing branch right there, which is exactly the cancellation a
    // dropped handler future (an operator's disconnect, a request timeout)
    // produces -- `release` is never notified, so the sweep could not have
    // finished on its own.
    tokio::select! {
        _ = gated.sweep("lucentroot", &state) => {
            panic!("the sweep must not finish: release is never notified in this test");
        }
        () = entered.notified() => {}
    }

    // Proved against a second, independently built service, not because
    // `gated` above would hang if reused: `Gated::component` parks only on
    // its *first* call (`first` is flipped to `false` before that call
    // parks), and that call has already been consumed by the cancelled
    // sweep, so a second `gated.sweep(...)` would now run to completion.
    // The fresh service exists to prove the *flag* is free, not to dodge
    // the fixture.
    let sane_desired_state = Arc::new(Several::new());
    let sane = service(&sane_desired_state);
    assert!(
        matches!(
            sane.sweep("lucentroot", &state).await.unwrap(),
            SweepResult::Ran(_)
        ),
        "the guard must be released after cancellation, not left AlreadyRunning forever"
    );
}

/// Desired state whose `components` call panics -- proving the guard is
/// released by `RunningGuard`'s `Drop`, not by code sequenced after an
/// `.await` that a panic unwinding through it never reaches.
struct PanickingComponents;

#[async_trait::async_trait]
impl DesiredState for PanickingComponents {
    async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
        panic!("simulated desired-state failure")
    }

    async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
        unreachable!("components() always panics first")
    }

    async fn advance(
        &self,
        _: &str,
        _: &str,
        _: &Release,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        unreachable!("components() always panics first")
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        _: &Release,
        _: &crate::Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        unreachable!("components() always panics first")
    }

    async fn pause(
        &self,
        _: &str,
        _: &str,
        _: &crate::Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        unreachable!("components() always panics first")
    }

    async fn resume(&self, _: &str, _: &str, _: &DesiredRevision, _: &str) -> Result<(), DesiredStateError> {
        unreachable!("components() always panics first")
    }
}

#[tokio::test]
async fn the_guard_releases_even_when_a_component_read_panics() {
    let state = Arc::new(SweepState::default());

    // Run inside a spawned task so the panic is caught as a `JoinError`
    // rather than aborting the test process -- what actually matters is
    // what it leaves behind in `state`, not how the panic itself surfaces.
    let panicked = tokio::spawn({
        let state = Arc::clone(&state);
        async move {
            let panicking = service_over(Arc::new(PanickingComponents) as Arc<dyn DesiredState>);
            panicking.sweep("lucentroot", &state).await
        }
    })
    .await;
    assert!(
        panicked.unwrap_err().is_panic(),
        "the join error must be a panic, not a cancellation, or this test would pass for either"
    );

    // Proved against a second, independently built service over the same
    // `state` -- the panicking one above is useless for a second call,
    // since its desired state panics unconditionally, but the guard it
    // held is `SweepState`'s, and this is the same state.
    let sane_desired_state = Arc::new(Several::new());
    let sane = service(&sane_desired_state);
    assert!(
        matches!(
            sane.sweep("lucentroot", &state).await.unwrap(),
            SweepResult::Ran(_)
        ),
        "the guard must be released after a panic, not left AlreadyRunning forever"
    );
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

        async fn advance(
            &self,
            _: &str,
            _: &str,
            _: &Release,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), DesiredStateError> {
            Err(DesiredStateError::NotConnected)
        }

        async fn roll_back(
            &self,
            _: &str,
            _: &str,
            _: &Release,
            _: &crate::Hold,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), DesiredStateError> {
            Err(DesiredStateError::NotConnected)
        }

        async fn pause(
            &self,
            _: &str,
            _: &str,
            _: &crate::Hold,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), DesiredStateError> {
            Err(DesiredStateError::NotConnected)
        }

        async fn resume(
            &self,
            _: &str,
            _: &str,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), DesiredStateError> {
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
