//! The two operations, and the difference between them.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use super::PlatformManagement;
use crate::{
    Channel, ComponentDesired, DesiredState, DesiredStateError, DesiredStateStatus, Hold, Provenance,
    Registry, RegistryError, ReleaseUnit, Resolved, UpdatePolicy, Version,
};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const CONSOLE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane-ui";

fn version(text: &str) -> Version {
    Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
}

/// A registry holding whole release units.
#[derive(Default)]
struct Registries {
    published: Mutex<BTreeMap<(String, String), Resolved>>,
}

impl Registries {
    fn publish_all(&self, tag: &str, revision: &str) {
        for repository in [RUNTIME, CONSOLE] {
            self.published
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(
                    (repository.to_owned(), tag.to_owned()),
                    Resolved {
                        digest: format!("sha256:{}", tag.len()),
                        provenance: Provenance::Agreed(revision.to_owned()),
                    },
                );
        }
    }
}

#[async_trait::async_trait]
impl Registry for Registries {
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        Ok(self
            .published
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .filter(|(published, _)| published == repository)
            .map(|(_, tag)| tag.clone())
            .collect())
    }

    async fn resolve(&self, repository: &str, tag: &str) -> Result<Option<Resolved>, RegistryError> {
        Ok(self
            .published
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(repository.to_owned(), tag.to_owned()))
            .cloned())
    }
}

/// Desired state that records every write it is asked for.
struct Recorded {
    desired: Mutex<ComponentDesired>,
    writes: Mutex<Vec<ReleaseUnit>>,
    refuse: Option<DesiredStateError>,
}

impl Recorded {
    fn new(policy: UpdatePolicy, hold: Option<Hold>) -> Self {
        Self {
            desired: Mutex::new(ComponentDesired {
                version: version("0.3.0-preview.2"),
                channel: Channel::Preview,
                policy,
                hold,
                repositories: BTreeMap::from([
                    ("console".to_owned(), CONSOLE.to_owned()),
                    ("runtime".to_owned(), RUNTIME.to_owned()),
                ]),
            }),
            writes: Mutex::new(Vec::new()),
            refuse: None,
        }
    }

    fn refusing(mut self, error: DesiredStateError) -> Self {
        self.refuse = Some(error);
        self
    }

    fn writes(&self) -> Vec<ReleaseUnit> {
        self.writes.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

#[async_trait::async_trait]
impl DesiredState for Recorded {
    async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
        Ok(self
            .desired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }

    async fn advance(&self, _: &str, _: &str, unit: &ReleaseUnit, _: &str) -> Result<(), DesiredStateError> {
        if let Some(error) = &self.refuse {
            return Err(error.clone());
        }

        self.writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(unit.clone());
        self.desired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .version = unit.version.clone();

        Ok(())
    }
}

/// A registry where `preview.3` is a complete release.
fn registries_with_three() -> Arc<Registries> {
    let registries = Arc::new(Registries::default());
    registries.publish_all("0.3.0-preview.2", "aaaa");
    registries.publish_all("0.3.0-preview.3", "bbbb");
    registries
}

/// The service over these two fakes, with the trait coercions in one place.
fn service(registries: &Arc<Registries>, desired_state: &Arc<Recorded>) -> PlatformManagement {
    PlatformManagement::new(
        Arc::clone(registries) as Arc<dyn Registry>,
        Arc::clone(desired_state) as Arc<dyn DesiredState>,
    )
}

fn held() -> Hold {
    Hold {
        reason: "rollback".to_owned(),
        since: "2026-09-01T09:00:00Z".to_owned(),
        note: None,
    }
}

#[tokio::test]
async fn reading_a_component_never_writes() {
    // The contract the console depends on. Opening a page, refreshing it, or a
    // second operator looking at the same screen must not move an
    // environment -- so `status` has no path to a write at all, whatever the
    // policy says.
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_three(), &desired_state);

    let status = service.status("lucentroot", "saas-fabric").await.unwrap();

    assert_eq!(status.desired, version("0.3.0-preview.2"));
    assert_eq!(status.available, Some(version("0.3.0-preview.3")));
    assert_eq!(status.desired_state, DesiredStateStatus::UpdateAvailable);
    assert!(
        desired_state.writes().is_empty(),
        "a read moved the environment: {:?}",
        desired_state.writes()
    );
}

#[tokio::test]
async fn reconciling_an_automatic_component_advances_it_once() {
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_three(), &desired_state);

    let status = service.reconcile("lucentroot", "saas-fabric").await.unwrap();

    let writes = desired_state.writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].version, version("0.3.0-preview.3"));
    assert_eq!(writes[0].source_revision, "bbbb");
    assert_eq!(writes[0].images.len(), 2, "a release unit moves whole");

    // The status describes what this call caused, not what a second read would
    // find -- which would race the reconciliation it just performed.
    assert_eq!(status.desired, version("0.3.0-preview.3"));
    assert_eq!(status.desired_state, DesiredStateStatus::Current);

    // And it settles: nothing newer, so a second pass writes nothing.
    service.reconcile("lucentroot", "saas-fabric").await.unwrap();
    assert_eq!(desired_state.writes().len(), 1, "reconciling twice wrote twice");
}

#[tokio::test]
async fn reconciling_a_held_component_writes_nothing_and_still_reports_the_update() {
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, Some(held())));
    let service = service(&registries_with_three(), &desired_state);

    let status = service.reconcile("lucentroot", "saas-fabric").await.unwrap();

    assert!(desired_state.writes().is_empty());
    assert!(status.is_paused(), "Automatic + hold reads as paused");
    assert_eq!(
        status.policy,
        UpdatePolicy::Automatic,
        "a hold is not a policy change"
    );
    assert_eq!(
        status.available,
        Some(version("0.3.0-preview.3")),
        "discovery keeps running while a hold stands"
    );
    assert_eq!(status.desired_state, DesiredStateStatus::UpdateAvailable);
}

#[tokio::test]
async fn reconciling_a_manual_component_writes_nothing() {
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Manual, None));
    let service = service(&registries_with_three(), &desired_state);

    let status = service.reconcile("lucentroot", "saas-fabric").await.unwrap();

    assert!(desired_state.writes().is_empty());
    assert!(!status.is_paused(), "manual is not paused, it is manual");
    assert_eq!(status.desired_state, DesiredStateStatus::UpdateAvailable);
}

#[tokio::test]
async fn automatic_advancement_stays_on_the_line_it_is_already_on() {
    // The series is the desired version's own core, so an automatic policy
    // walks forward within 0.3.0 and does not wander onto 0.4.0. Moving a line
    // is a deliberate act -- a decision about what an environment is for --
    // not something discovery does on a Tuesday because a tag appeared.
    let registries = Arc::new(Registries::default());
    registries.publish_all("0.3.0-preview.2", "aaaa");
    registries.publish_all("0.3.0-preview.3", "bbbb");
    registries.publish_all("0.4.0-preview.1", "cccc");
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries, &desired_state);

    let status = service.reconcile("lucentroot", "saas-fabric").await.unwrap();

    assert_eq!(status.desired, version("0.3.0-preview.3"));
    assert_eq!(
        desired_state.writes()[0].version,
        version("0.3.0-preview.3"),
        "an automatic policy walked onto another release line"
    );
    assert!(
        !status.diagnostics.not_yet.contains(&version("0.4.0-preview.1")),
        "another line is not a diagnostic, it is simply not eligible"
    );
}

#[tokio::test]
async fn a_component_with_nothing_newer_is_current() {
    let registries = Arc::new(Registries::default());
    registries.publish_all("0.3.0-preview.2", "aaaa");
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries, &desired_state);

    let status = service.reconcile("lucentroot", "saas-fabric").await.unwrap();

    assert_eq!(status.available, None);
    assert_eq!(status.desired_state, DesiredStateStatus::Current);
    assert!(desired_state.writes().is_empty());
}

#[tokio::test]
async fn a_conflicting_write_is_reported_rather_than_retried_here() {
    // The decision was taken against desired state that has since moved. That
    // is an instruction to decide again, and deciding again is the caller's --
    // retrying inside this would be deciding twice from one read.
    let desired_state =
        Arc::new(Recorded::new(UpdatePolicy::Automatic, None).refusing(DesiredStateError::Conflict));
    let service = service(&registries_with_three(), &desired_state);

    let failure = service
        .reconcile("lucentroot", "saas-fabric")
        .await
        .expect_err("conflict");

    assert!(matches!(
        failure,
        crate::PlatformError::DesiredState(DesiredStateError::Conflict)
    ));
    assert!(desired_state.writes().is_empty());
}

#[tokio::test]
async fn versions_that_were_not_selected_are_reported() {
    let registries = Arc::new(Registries::default());
    registries.publish_all("0.3.0-preview.2", "aaaa");
    // `.3` exists for the runtime only: still publishing.
    registries
        .published
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(
            (RUNTIME.to_owned(), "0.3.0-preview.3".to_owned()),
            Resolved {
                digest: "sha256:partial".to_owned(),
                provenance: Provenance::Agreed("bbbb".to_owned()),
            },
        );

    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries, &desired_state);

    let status = service.status("lucentroot", "saas-fabric").await.unwrap();

    assert_eq!(status.diagnostics.not_yet, vec![version("0.3.0-preview.3")]);
    assert_eq!(status.available, None, "an incomplete release is not available");
    assert_eq!(status.desired_state, DesiredStateStatus::Current);
}
