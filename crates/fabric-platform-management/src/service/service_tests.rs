//! The two operations, and the difference between them.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use super::{PlatformError, PlatformManagement};
use crate::{
    ArtifactSource, Channel, ChartIndex, ComponentDesired, DesiredRevision, DesiredState, DesiredStateError,
    DesiredStateStatus, Hold, Provenance, Registry, RegistryError, Release, ReleaseUnit, Resolved,
    UpdatePolicy, Version,
};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const CONSOLE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane-ui";

fn version(text: &str) -> Version {
    Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
}

/// A registry holding whole release units.
///
/// Counts resolutions, because how *many* a request makes is a property worth
/// keeping: the difference between resolving one version and re-deriving a
/// listing is the difference between an answer and a gateway timeout.
#[derive(Default)]
struct Registries {
    published: Mutex<BTreeMap<(String, String), Resolved>>,
    resolutions: Mutex<usize>,
}

impl Registries {
    fn forget_resolutions(&self) {
        *self.resolutions.lock().unwrap_or_else(PoisonError::into_inner) = 0;
    }

    fn resolutions(&self) -> usize {
        *self.resolutions.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// One image only, which is how a version fails to be a release unit.
    fn publish(&self, repository: &str, tag: &str, revision: &str) {
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
        *self.resolutions.lock().unwrap_or_else(PoisonError::into_inner) += 1;

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
    writes: Mutex<Vec<Release>>,
    /// Every hold written, `None` meaning one was lifted. Separate from
    /// `writes` because the guarantee under test is that these two paths never
    /// become each other.
    holds: Mutex<Vec<Option<Hold>>>,
    /// Every rollback written, as the unit and the hold that travelled with
    /// it. A third list, because the guarantee is that these three write paths
    /// never become each other.
    rollbacks: Mutex<Vec<(ReleaseUnit, Hold)>>,
    refuse: Option<DesiredStateError>,
}

impl Recorded {
    fn new(policy: UpdatePolicy, hold: Option<Hold>) -> Self {
        Self::at("0.3.0-preview.2", policy, hold)
    }

    /// The same fixture, running a stated version.
    ///
    /// Needed because candidates are filtered by *series* as well as channel,
    /// so a test about how far back a listing reaches has to put the desired
    /// version far enough forward for there to be a back.
    fn at(running: &str, policy: UpdatePolicy, hold: Option<Hold>) -> Self {
        Self {
            desired: Mutex::new(ComponentDesired {
                revision: DesiredRevision::new("read-1"),
                version: version(running),
                channel: Channel::Preview,
                policy,
                hold,
                source: ArtifactSource::Oci {
                    repositories: BTreeMap::from([
                        ("console".to_owned(), CONSOLE.to_owned()),
                        ("runtime".to_owned(), RUNTIME.to_owned()),
                    ]),
                },
            }),
            writes: Mutex::new(Vec::new()),
            holds: Mutex::new(Vec::new()),
            rollbacks: Mutex::new(Vec::new()),
            refuse: None,
        }
    }

    fn refusing(mut self, error: DesiredStateError) -> Self {
        self.refuse = Some(error);
        self
    }

    fn writes(&self) -> Vec<Release> {
        self.writes.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn rollbacks(&self) -> Vec<(ReleaseUnit, Hold)> {
        self.rollbacks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn holds(&self) -> Vec<Option<Hold>> {
        self.holds.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn current(&self) -> ComponentDesired {
        self.desired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl DesiredState for Recorded {
    async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
        Ok(vec!["saas-fabric".to_owned()])
    }

    async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
        Ok(self
            .desired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }

    async fn advance(
        &self,
        _: &str,
        _: &str,
        release: &Release,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        if let Some(error) = &self.refuse {
            return Err(error.clone());
        }

        self.writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(release.clone());
        self.desired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .version = release.version().clone();

        Ok(())
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        unit: &ReleaseUnit,
        hold: &Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        if let Some(error) = &self.refuse {
            return Err(error.clone());
        }

        self.rollbacks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((unit.clone(), hold.clone()));

        let mut desired = self.desired.lock().unwrap_or_else(PoisonError::into_inner);
        desired.version = unit.version.clone();
        desired.hold = Some(hold.clone());

        Ok(())
    }

    async fn pause(
        &self,
        _: &str,
        _: &str,
        hold: &Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        if let Some(error) = &self.refuse {
            return Err(error.clone());
        }

        self.holds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Some(hold.clone()));
        self.desired.lock().unwrap_or_else(PoisonError::into_inner).hold = Some(hold.clone());

        Ok(())
    }

    async fn resume(&self, _: &str, _: &str, _: &DesiredRevision, _: &str) -> Result<(), DesiredStateError> {
        if let Some(error) = &self.refuse {
            return Err(error.clone());
        }

        self.holds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(None);
        self.desired.lock().unwrap_or_else(PoisonError::into_inner).hold = None;

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
        Arc::new(Charts::default()) as Arc<dyn ChartIndex>,
        Arc::clone(desired_state) as Arc<dyn DesiredState>,
        Arc::new(fabric_core::SystemClock::new()) as Arc<dyn fabric_core::Clock>,
    )
}

fn held() -> Hold {
    Hold {
        reason: "rollback".to_owned(),
        since: "2026-09-01T09:00:00Z".to_owned(),
        note: None,
    }
}

/// A chart repository holding stated versions.
#[derive(Default)]
struct Charts {
    /// Versions by (repository, chart).
    published: Mutex<BTreeMap<(String, String), Vec<Version>>>,
}

impl Charts {
    fn publish(&self, repository: &str, chart: &str, versions: &[&str]) {
        self.published
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                (repository.to_owned(), chart.to_owned()),
                versions.iter().map(|text| version(text)).collect(),
            );
    }
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
async fn reading_a_component_never_writes() {
    // The contract the console depends on. Opening a page, refreshing it, or a
    // second operator looking at the same screen must not move an
    // environment -- so `status` has no path to a write at all, whatever the
    // policy says.
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_three(), &desired_state);

    let status = service.status("lucentroot", "saas-fabric").await.unwrap();

    assert_eq!(status.desired, version("0.3.0-preview.2"));
    assert_eq!(status.newer, Some(version("0.3.0-preview.3")));
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

    let status = service
        .reconcile("lucentroot", "saas-fabric")
        .await
        .unwrap()
        .status;

    let writes = desired_state.writes();
    assert_eq!(writes.len(), 1);
    let Release::Unit(written) = &writes[0] else {
        panic!("images advance as a release unit");
    };
    assert_eq!(written.version, version("0.3.0-preview.3"));
    assert_eq!(written.source_revision, "bbbb");
    assert_eq!(written.images.len(), 2, "a release unit moves whole");

    // The status describes what this call caused, not what a second read would
    // find -- which would race the reconciliation it just performed.
    assert_eq!(status.desired, version("0.3.0-preview.3"));
    assert_eq!(status.desired_state, DesiredStateStatus::Current);

    // And it settles: nothing newer, so a second pass writes nothing.
    let settled = service.reconcile("lucentroot", "saas-fabric").await.unwrap();
    assert!(!settled.advanced(), "a settled component reported an advance");
    assert_eq!(desired_state.writes().len(), 1, "reconciling twice wrote twice");
}

#[tokio::test]
async fn reconciling_a_held_component_writes_nothing_and_still_reports_the_update() {
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, Some(held())));
    let service = service(&registries_with_three(), &desired_state);

    let status = service
        .reconcile("lucentroot", "saas-fabric")
        .await
        .unwrap()
        .status;

    assert!(desired_state.writes().is_empty());
    assert!(status.is_paused(), "Automatic + hold reads as paused");
    assert_eq!(
        status.policy,
        UpdatePolicy::Automatic,
        "a hold is not a policy change"
    );
    assert_eq!(
        status.newer,
        Some(version("0.3.0-preview.3")),
        "discovery keeps running while a hold stands"
    );
    assert_eq!(status.desired_state, DesiredStateStatus::UpdateAvailable);
}

#[tokio::test]
async fn reconciling_a_manual_component_writes_nothing() {
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Manual, None));
    let service = service(&registries_with_three(), &desired_state);

    let status = service
        .reconcile("lucentroot", "saas-fabric")
        .await
        .unwrap()
        .status;

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

    let status = service
        .reconcile("lucentroot", "saas-fabric")
        .await
        .unwrap()
        .status;

    assert_eq!(status.desired, version("0.3.0-preview.3"));
    assert_eq!(
        desired_state.writes()[0].version().clone(),
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

    let status = service
        .reconcile("lucentroot", "saas-fabric")
        .await
        .unwrap()
        .status;

    assert_eq!(status.newer, None);
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
    assert_eq!(
        status.newer, None,
        "an incomplete release is nothing to advance to"
    );
    assert_eq!(status.desired_state, DesiredStateStatus::Current);
}

// ---------------------------------------------------------------------------
// The brake.
//
// Pause and resume are what an *operator* does, and the guarantee worth having
// is that they and the selector never become each other: a sweep cannot lift a
// hold to succeed, and a pause cannot move a version.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pausing_writes_a_hold_and_moves_nothing() {
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_three(), &desired_state);

    let status = service
        .pause("lucentroot", "saas-fabric", Some("testing preview.3 by hand"))
        .await
        .expect("pausing must succeed");

    assert!(
        desired_state.writes().is_empty(),
        "pausing must not move a version: {:?}",
        desired_state.writes()
    );
    assert_eq!(
        desired_state.current().version,
        version("0.3.0-preview.2"),
        "the environment runs what it ran"
    );
    assert!(status.is_paused(), "Automatic + hold reads as paused");
    assert_eq!(
        status.policy,
        UpdatePolicy::Automatic,
        "a pause is not a policy change; the operator did not say 'stop forever'"
    );
}

#[tokio::test]
async fn a_pause_carries_the_operators_note_and_a_reason_they_did_not_choose() {
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_three(), &desired_state);

    service
        .pause("lucentroot", "saas-fabric", Some("waiting on the Secrets fix"))
        .await
        .expect("pausing must succeed");

    let written = desired_state.holds();
    let hold = written
        .first()
        .and_then(Option::as_ref)
        .expect("a hold must have been written");

    // The reason is a closed vocabulary, because a later reader decides from
    // it. The note is the operator's, because nothing branches on it.
    assert_eq!(hold.reason, "paused");
    assert_eq!(hold.note.as_deref(), Some("waiting on the Secrets fix"));
    assert!(!hold.since.is_empty(), "a hold records when it started");
}

#[tokio::test]
async fn a_component_that_does_not_advance_cannot_be_paused() {
    // Recording a hold on a Manual component would put a pause in the manifest
    // that stops nothing, and show an operator "Paused" about something that
    // was never moving.
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Manual, None));
    let service = service(&registries_with_three(), &desired_state);

    let outcome = service.pause("lucentroot", "saas-fabric", None).await;

    assert!(
        matches!(outcome, Err(PlatformError::NotAdvancing { .. })),
        "a manual component has nothing to pause"
    );
    assert!(desired_state.holds().is_empty());
}

#[tokio::test]
async fn resuming_lifts_the_hold_and_advances_nothing() {
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, Some(held())));
    let service = service(&registries_with_three(), &desired_state);

    let status = service
        .resume("lucentroot", "saas-fabric")
        .await
        .expect("resuming must succeed");

    assert_eq!(desired_state.holds(), vec![None], "one write, and it is the lift");
    assert!(
        desired_state.writes().is_empty(),
        "resuming permits advancement; the next sweep decides it"
    );
    assert!(status.hold.is_none());
    assert_eq!(
        desired_state.current().version,
        version("0.3.0-preview.2"),
        "the version an operator resumes from is the version they were on"
    );
}

#[tokio::test]
async fn resuming_something_that_is_not_held_writes_no_commit() {
    // The repository's history is the audit trail. An empty commit would
    // record that somebody clicked, not that anything happened.
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_three(), &desired_state);

    service
        .resume("lucentroot", "saas-fabric")
        .await
        .expect("resuming an unheld component is not an error");

    assert!(desired_state.holds().is_empty());
    assert!(desired_state.writes().is_empty());
}

#[tokio::test]
async fn pausing_reports_nothing_about_what_is_newer() {
    // It asked no registry anything. Reporting a newer version here would be
    // stating something this pass did not observe -- and the row would go
    // stale the moment a preview was published.
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_three(), &desired_state);

    let status = service
        .pause("lucentroot", "saas-fabric", None)
        .await
        .expect("pausing must succeed");

    assert_eq!(status.newer, None);
}

#[tokio::test]
async fn a_component_the_manifest_does_not_name_cannot_be_paused() {
    // The identifier rule. An operator may select something the environment's
    // manifest already names; the name is a lookup key and reaches nothing
    // else, so a value shaped like a path is simply not a component.
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None).refusing(
        DesiredStateError::NotFound {
            what: "../../etc in lucentroot".to_owned(),
        },
    ));
    let service = service(&registries_with_three(), &desired_state);

    let outcome = service.pause("lucentroot", "../../etc", None).await;

    assert!(
        matches!(
            outcome,
            Err(PlatformError::DesiredState(DesiredStateError::NotFound { .. }))
        ),
        "a name the manifest does not carry selects nothing"
    );
    assert!(desired_state.holds().is_empty());
}

// ---------------------------------------------------------------------------
// Rolling back.
//
// The contract: an operator names a version the platform already observed as a
// complete coherent release unit, and the version, its three digests and a
// rollback hold move together in one commit.
// ---------------------------------------------------------------------------

/// A registry holding two releases below the desired one, and one broken.
fn registries_with_history() -> Arc<Registries> {
    let registries = Arc::new(Registries::default());
    registries.publish_all("0.3.0-preview.1", "aaaa");
    registries.publish_all("0.3.0-preview.2", "bbbb");
    // Only one of its images was ever published, so it is not a release unit.
    registries.publish(RUNTIME, "0.3.0-preview.15", "cccc");
    registries
}

#[tokio::test]
async fn only_versions_that_were_whole_releases_are_offered() {
    // Desired is preview.2, so preview.1 is below it. preview.15 is above it
    // *and* incomplete, and belongs in neither list.
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_history(), &desired_state);

    let found = service
        .rollback_candidates("lucentroot", "saas-fabric")
        .await
        .expect("candidates must be readable");

    assert_eq!(
        found
            .units
            .iter()
            .map(|unit| unit.version.as_str().to_owned())
            .collect::<Vec<_>>(),
        vec!["0.3.0-preview.1".to_owned()],
        "only complete coherent releases below the desired one"
    );
    assert!(!found.more, "two candidates is not a truncated list");
}

#[tokio::test]
async fn the_desired_version_is_not_something_to_roll_back_to() {
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_history(), &desired_state);

    let found = service
        .rollback_candidates("lucentroot", "saas-fabric")
        .await
        .expect("candidates must be readable");

    assert!(
        !found
            .units
            .iter()
            .any(|unit| unit.version == version("0.3.0-preview.2")),
        "the version already running is not a destination"
    );
}

#[tokio::test]
async fn rolling_back_moves_the_version_and_holds_it_in_one_commit() {
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_history(), &desired_state);

    let status = service
        .roll_back(
            "lucentroot",
            "saas-fabric",
            "0.3.0-preview.1",
            Some("broke Secrets"),
        )
        .await
        .expect("the rollback must succeed");

    assert_eq!(status.desired, version("0.3.0-preview.1"));
    assert!(status.is_paused(), "an environment put back stays put");
    assert_eq!(
        status.policy,
        UpdatePolicy::Automatic,
        "rolling back is not a decision to stop advancing forever"
    );

    let hold = status.hold.as_ref().expect("a rollback records a hold");
    assert_eq!(hold.reason, "rollback", "distinct from a plain pause");
    assert_eq!(hold.note.as_deref(), Some("broke Secrets"));
}

#[tokio::test]
async fn a_version_the_platform_never_observed_is_refused() {
    // The whole of "no free-text version entry": a caller may name one, and a
    // name that does not match an observed release unit writes nothing.
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_history(), &desired_state);

    for asked in ["0.3.0-preview.99", "not-a-version", "0.3.0-preview.15"] {
        let outcome = service.roll_back("lucentroot", "saas-fabric", asked, None).await;

        assert!(
            matches!(outcome, Err(PlatformError::NotRollable { .. })),
            "{asked} must not be rollable"
        );
    }

    assert!(desired_state.rollbacks().is_empty());
}

#[tokio::test]
async fn rolling_back_writes_the_digests_the_platform_resolved() {
    // Not the caller's, because the caller sends none. The unit written is the
    // one discovery assembled from the registry.
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries_with_history(), &desired_state);

    service
        .roll_back("lucentroot", "saas-fabric", "0.3.0-preview.1", None)
        .await
        .expect("the rollback must succeed");

    let written = desired_state.rollbacks();
    let (unit, hold) = written.first().expect("one rollback was written");

    assert_eq!(unit.version, version("0.3.0-preview.1"));
    assert_eq!(
        unit.source_revision, "aaaa",
        "the commit its images were built from"
    );
    assert_eq!(unit.images.len(), 2, "every image moves, not one");
    assert_eq!(hold.reason, "rollback");
}

#[tokio::test]
async fn resuming_after_a_rollback_selects_the_newest_and_not_what_it_came_from() {
    // The property that says rollback history is not a queue. An environment
    // rolled back from preview.2 to preview.1, with preview.3 and preview.4
    // published since, must resume onto preview.4 -- the newest eligible
    // coherent release -- rather than replaying the version it came from.
    //
    // Nothing remembers a rollback. The selector reads the floor from desired
    // state and asks the registry what is above it, so "what we came from" is
    // not something it could prefer even if somebody wanted it to.
    let registries = Arc::new(Registries::default());
    registries.publish_all("0.3.0-preview.1", "aaaa");
    registries.publish_all("0.3.0-preview.2", "bbbb");
    registries.publish_all("0.3.0-preview.3", "cccc");
    registries.publish_all("0.3.0-preview.4", "dddd");

    // The fixture runs preview.2.
    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let service = service(&registries, &desired_state);

    service
        .roll_back(
            "lucentroot",
            "saas-fabric",
            "0.3.0-preview.1",
            Some("broke Secrets"),
        )
        .await
        .expect("the rollback must succeed");

    assert_eq!(desired_state.current().version, version("0.3.0-preview.1"));

    // Held, so a sweep changes nothing however much is newer.
    service
        .reconcile("lucentroot", "saas-fabric")
        .await
        .expect("a held reconcile is not an error");

    assert!(
        desired_state.writes().is_empty(),
        "a rollback hold stops advancement: {:?}",
        desired_state.writes()
    );

    service
        .resume("lucentroot", "saas-fabric")
        .await
        .expect("resuming must succeed");

    let status = service
        .reconcile("lucentroot", "saas-fabric")
        .await
        .expect("the sweep after a resume must run")
        .status;

    assert_eq!(
        status.desired,
        version("0.3.0-preview.4"),
        "the newest eligible release, not the one the rollback came from"
    );
    assert_eq!(
        desired_state
            .writes()
            .iter()
            .map(|release| release.version().as_str().to_owned())
            .collect::<Vec<_>>(),
        vec!["0.3.0-preview.4".to_owned()],
        "one advance, and it skips preview.2 and preview.3 entirely"
    );
}

#[tokio::test]
async fn a_version_older_than_the_listing_bound_is_still_rollable() {
    // The bound limits what is *offered*, not what is valid. A safety rule
    // that made a real release unrollable because five newer ones existed
    // would be a strange one, and an operator who knows the version they want
    // should not be told it never existed.
    let registries = Arc::new(Registries::default());
    for n in 1..=8 {
        registries.publish_all(&format!("0.3.0-preview.{n}"), "aaaa");
    }

    let desired_state = Arc::new(Recorded::at("0.3.0-preview.8", UpdatePolicy::Automatic, None));
    let service = service(&registries, &desired_state);

    let offered = service
        .rollback_candidates("lucentroot", "saas-fabric")
        .await
        .expect("candidates must be readable");

    assert_eq!(offered.units.len(), 5, "the listing stops at five");
    assert!(offered.more, "and says there are more");
    assert!(
        !offered
            .units
            .iter()
            .any(|unit| unit.version == version("0.3.0-preview.1")),
        "preview.1 is past the bound and not offered"
    );

    // And is still a version this component can be put back on.
    let status = service
        .roll_back("lucentroot", "saas-fabric", "0.3.0-preview.1", None)
        .await
        .expect("a real release below the desired one is rollable");

    assert_eq!(status.desired, version("0.3.0-preview.1"));
}

#[tokio::test]
async fn rolling_back_asks_the_registry_about_one_version_and_not_the_whole_history() {
    // Not a style preference. Re-deriving the listing to check membership made
    // an operator's click pay for five versions plus a Git write, which
    // exceeded the request budget against a real registry and answered 504.
    let registries = Arc::new(Registries::default());
    for n in 1..=6 {
        registries.publish_all(&format!("0.3.0-preview.{n}"), "aaaa");
    }

    let desired_state = Arc::new(Recorded::at("0.3.0-preview.6", UpdatePolicy::Automatic, None));
    let service = service(&registries, &desired_state);

    registries.forget_resolutions();

    service
        .roll_back("lucentroot", "saas-fabric", "0.3.0-preview.5", None)
        .await
        .expect("the rollback must succeed");

    assert_eq!(
        registries.resolutions(),
        2,
        "one call per image of the one version asked for, and no listing pass"
    );
}

// ---------------------------------------------------------------------------
// Charts.
//
// A second artifact kind, discovered differently and guaranteeing less. What
// these pin is that the difference is visible rather than papered over.
// ---------------------------------------------------------------------------

const HELM: &str = "https://codecentric.github.io/helm-charts";

/// A component published as a chart, running 7.3.0.
fn charted(policy: UpdatePolicy) -> Arc<Recorded> {
    let mut fixture = Recorded::at("7.3.0", policy, None);
    fixture.desired = Mutex::new(ComponentDesired {
        revision: DesiredRevision::new("read-1"),
        version: version("7.3.0"),
        channel: Channel::Stable,
        policy,
        hold: None,
        source: ArtifactSource::Helm {
            repository: HELM.to_owned(),
            chart: "keycloakx".to_owned(),
        },
    });

    Arc::new(fixture)
}

/// The service over a chart repository holding stated versions.
fn charted_service(charts: &Arc<Charts>, desired_state: &Arc<Recorded>) -> PlatformManagement {
    PlatformManagement::new(
        Arc::new(Registries::default()) as Arc<dyn Registry>,
        Arc::clone(charts) as Arc<dyn ChartIndex>,
        Arc::clone(desired_state) as Arc<dyn DesiredState>,
        Arc::new(fabric_core::SystemClock::new()) as Arc<dyn fabric_core::Clock>,
    )
}

#[tokio::test]
async fn a_chart_reports_the_newest_version_its_repository_publishes() {
    let charts = Arc::new(Charts::default());
    charts.publish(HELM, "keycloakx", &["7.2.3", "7.3.0", "7.3.1"]);

    let desired_state = charted(UpdatePolicy::Manual);
    let status = charted_service(&charts, &desired_state)
        .status("lucentroot", "keycloak")
        .await
        .expect("a chart component reads");

    assert_eq!(status.desired, version("7.3.0"));
    assert_eq!(status.newer, Some(version("7.3.1")));
    assert_eq!(status.desired_state, DesiredStateStatus::UpdateAvailable);
}

#[tokio::test]
async fn a_chart_reports_no_diagnostics_because_it_has_none_to_report() {
    // `not_yet` and `incoherent` exist because several images can be
    // half-published or built twice. A chart is one artifact: it cannot be
    // partly there and cannot disagree with itself.
    let charts = Arc::new(Charts::default());
    charts.publish(HELM, "keycloakx", &["7.3.0", "7.3.1"]);

    let status = charted_service(&charts, &charted(UpdatePolicy::Manual))
        .status("lucentroot", "keycloak")
        .await
        .expect("a chart component reads");

    assert!(status.diagnostics.not_yet.is_empty());
    assert!(status.diagnostics.incoherent.is_empty());
}

#[tokio::test]
async fn a_stable_component_on_automatic_advances_nothing_and_says_why() {
    // Fail closed. A prerelease advances within its line and the line stops it
    // crossing to another; a stable version has no line, so nothing would stop
    // 7.3.0 becoming 8.0.0 on a sweep at three in the morning.
    //
    // Which upgrades an automatic stable policy may take is undecided, and the
    // safe answer to an undecided rule is to do nothing and say so -- not to
    // do whatever the code happens to permit.
    let charts = Arc::new(Charts::default());
    charts.publish(HELM, "keycloakx", &["7.3.0", "7.3.1", "8.0.0"]);

    let desired_state = charted(UpdatePolicy::Automatic);
    let status = charted_service(&charts, &desired_state)
        .reconcile("lucentroot", "keycloak")
        .await
        .expect("a reconcile that declines is not an error")
        .status;

    assert!(
        desired_state.writes().is_empty(),
        "nothing advances until the policy is defined: {:?}",
        desired_state.writes()
    );

    // And the operator is still told what exists, so the decision they have to
    // make is visible rather than hidden behind a component that looks idle.
    assert_eq!(status.newer, Some(version("8.0.0")));
}

#[tokio::test]
async fn a_chart_cannot_be_rolled_back_and_says_why() {
    // Not "not implemented yet". A chart repository pins a version, and the
    // bytes behind it can be republished -- so "put me back on what I was
    // running" is a promise it cannot keep, and offering the control would be
    // offering a guarantee this platform does not have.
    let charts = Arc::new(Charts::default());
    charts.publish(HELM, "keycloakx", &["7.2.0", "7.3.0", "7.3.1"]);

    let service = charted_service(&charts, &charted(UpdatePolicy::Manual));

    let listing = service.rollback_candidates("lucentroot", "keycloak").await;
    assert!(
        matches!(listing, Err(PlatformError::RollbackUnsupported { .. })),
        "the listing refuses too: an empty list would say there is nowhere to go, \
         which is a different claim"
    );

    let attempt = service.roll_back("lucentroot", "keycloak", "7.2.0", None).await;
    assert!(
        matches!(attempt, Err(PlatformError::RollbackUnsupported { .. })),
        "{attempt:?}"
    );
}

#[tokio::test]
async fn a_chart_can_still_be_paused_and_resumed() {
    // Stopping an environment advancing needs no immutability at all, so the
    // brake is offered for both kinds even though rollback is not.
    let charts = Arc::new(Charts::default());
    charts.publish(HELM, "keycloakx", &["7.3.0", "7.3.1"]);

    let desired_state = charted(UpdatePolicy::Automatic);
    let service = charted_service(&charts, &desired_state);

    let paused = service
        .pause("lucentroot", "keycloak", Some("watching 7.3.1 elsewhere first"))
        .await
        .expect("a chart pauses");

    assert!(paused.is_paused());
    assert!(!paused.rollable, "and still cannot be rolled back");
    assert!(desired_state.writes().is_empty(), "pausing moves no version");

    service
        .resume("lucentroot", "keycloak")
        .await
        .expect("a chart resumes");
}

#[tokio::test]
async fn a_stable_component_can_advance_at_all() {
    // The defect this found. The series rule -- "an automatic policy walks
    // forward within the desired version's own line" -- is `core == core`, and
    // every stable advance changes the core. Applied to a stable component it
    // meant nothing was ever newer, however much its repository published, and
    // the console would have said so with a dash.
    //
    // Nothing had noticed because the only managed component was a preview.
    let charts = Arc::new(Charts::default());
    charts.publish(HELM, "keycloakx", &["7.3.0", "7.3.1"]);

    let status = charted_service(&charts, &charted(UpdatePolicy::Manual))
        .status("lucentroot", "keycloak")
        .await
        .expect("a stable component reads");

    assert_eq!(
        status.newer,
        Some(version("7.3.1")),
        "7.3.1 is newer than 7.3.0 and is not a different line"
    );
}

#[tokio::test]
async fn a_preview_still_stays_on_the_line_it_is_on() {
    // The rule that made the one above look correct. It is right for a
    // prerelease and only for a prerelease: 0.4.0-preview.1 is a different
    // line, and moving to it is a deliberate act.
    let registries = Arc::new(Registries::default());
    registries.publish_all("0.3.0-preview.3", "aaaa");
    registries.publish_all("0.4.0-preview.1", "bbbb");

    let desired_state = Arc::new(Recorded::new(UpdatePolicy::Automatic, None));
    let status = service(&registries, &desired_state)
        .status("lucentroot", "saas-fabric")
        .await
        .expect("reads");

    assert_eq!(
        status.newer,
        Some(version("0.3.0-preview.3")),
        "the next preview of this line, not the first of the next one"
    );
}
