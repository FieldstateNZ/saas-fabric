//! A sweep looks after every component, and keeps going when one cannot be.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::{
    Channel, ComponentDesired, DesiredState, DesiredStateError, PlatformManagement, Provenance, Registry,
    RegistryError, ReleaseUnit, Resolved, SweepGuard, SweepResult, Swept, UpdatePolicy, Version,
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
                repositories: BTreeMap::from([("runtime".to_owned(), RUNTIME.to_owned())]),
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
        unit: &ReleaseUnit,
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
            desired.version = unit.version.clone();
        }

        Ok(())
    }
}

fn service(desired_state: &Arc<Several>) -> PlatformManagement {
    PlatformManagement::new(
        Arc::new(Registries) as Arc<dyn Registry>,
        Arc::clone(desired_state) as Arc<dyn DesiredState>,
    )
}

#[tokio::test]
async fn one_component_failing_does_not_stop_the_others() {
    // `a-broken` sorts first, so a sweep that gave up on the first failure
    // would leave every component after it permanently unreconciled -- in an
    // order nobody chose.
    let desired_state = Arc::new(Several::new());
    let service = service(&desired_state);

    let SweepResult::Ran(sweep) = service.sweep("lucentroot", &SweepGuard::default()).await.unwrap() else {
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

    let SweepResult::Ran(sweep) = service.sweep("lucentroot", &SweepGuard::default()).await.unwrap() else {
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

    let SweepResult::Ran(sweep) = service.sweep("lucentroot", &SweepGuard::default()).await.unwrap() else {
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
    let guard = SweepGuard::default();

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

    async fn advance(&self, e: &str, c: &str, unit: &ReleaseUnit, m: &str) -> Result<(), DesiredStateError> {
        self.inner.advance(e, c, unit, m).await
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
        Arc::clone(&desired_state) as Arc<dyn DesiredState>,
    ));
    let guard = Arc::new(SweepGuard::default());

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
