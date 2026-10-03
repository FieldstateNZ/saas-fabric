//! A described component through the service: what it advances to, what it
//! rolls back to, what each commit says, and what the console is told.

use std::sync::{Arc, Mutex, PoisonError};

use super::PlatformManagement;
use crate::discovery::described_fake_registry::{descriptor_digest, pins, FakeRegistry};
use crate::{
    ArtifactKind, ArtifactSource, Attached, Channel, ChartIndex, ComponentDesired, DesiredRevision,
    DesiredState, DesiredStateError, Hold, InvalidReason, InvalidVersion, Registry, RegistryError, Release,
    UpdatePolicy, Version,
};

fn version(text: &str) -> Version {
    Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
}

/// Desired state for one described component, recording every commit
/// message a write was given.
struct Written {
    desired: Mutex<ComponentDesired>,
    messages: Mutex<Vec<(Release, String)>>,
}

impl Written {
    fn at(running: &str) -> Self {
        Self {
            desired: Mutex::new(ComponentDesired {
                revision: DesiredRevision::new("read-1"),
                version: version(running),
                channel: Channel::Preview,
                policy: UpdatePolicy::Automatic,
                hold: None,
                source: ArtifactSource::Described {
                    primary: "runtime".to_owned(),
                    repositories: pins(),
                },
            }),
            messages: Mutex::new(Vec::new()),
        }
    }

    fn messages(&self) -> Vec<(Release, String)> {
        self.messages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn record(&self, release: &Release, message: &str) {
        self.messages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((release.clone(), message.to_owned()));
    }
}

#[async_trait::async_trait]
impl DesiredState for Written {
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
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.record(release, message);
        Ok(())
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        release: &Release,
        _: &Hold,
        _: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.record(release, message);
        Ok(())
    }

    async fn pause(
        &self,
        _: &str,
        _: &str,
        _: &Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }

    async fn resume(&self, _: &str, _: &str, _: &DesiredRevision, _: &str) -> Result<(), DesiredStateError> {
        Ok(())
    }
}

/// No chart repository is asked about a described component.
struct NoCharts;

#[async_trait::async_trait]
impl ChartIndex for NoCharts {
    async fn versions(&self, _: &str, _: &str) -> Result<Vec<Version>, RegistryError> {
        panic!("a described component is not a chart");
    }
}

fn service(registry: &Arc<FakeRegistry>, desired: &Arc<Written>) -> PlatformManagement {
    PlatformManagement::new(
        Arc::clone(registry) as Arc<dyn Registry>,
        Arc::new(NoCharts) as Arc<dyn ChartIndex>,
        Arc::clone(desired) as Arc<dyn DesiredState>,
        Arc::new(fabric_core::SystemClock::new()) as Arc<dyn fabric_core::Clock>,
    )
}

const COMMIT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[tokio::test]
async fn an_advance_names_the_commit_and_the_component_descriptor() {
    let registry = Arc::new(FakeRegistry::default());
    registry.publish("0.3.0-preview.3", COMMIT);
    let desired = Arc::new(Written::at("0.3.0-preview.2"));

    let done = service(&registry, &desired)
        .reconcile("lucentroot", "saas-fabric")
        .await
        .expect("a described release advances");

    assert_eq!(done.status.desired.as_str(), "0.3.0-preview.3");
    let [(release, message)] = desired.messages().try_into().expect("one write");
    assert!(matches!(release, Release::Described { .. }), "{release:?}");
    assert_eq!(
        message,
        format!(
            "Advance lucentroot to saas-fabric 0.3.0-preview.3\n\nBuilt from {COMMIT}.\nComponent descriptor {}.",
            descriptor_digest("0.3.0-preview.3")
        )
    );
}

#[tokio::test]
async fn a_rollback_names_the_component_descriptor_it_re_resolved() {
    let registry = Arc::new(FakeRegistry::default());
    registry.publish("0.3.0-preview.1", COMMIT);
    let desired = Arc::new(Written::at("0.3.0-preview.2"));

    service(&registry, &desired)
        .roll_back("lucentroot", "saas-fabric", "0.3.0-preview.1", None)
        .await
        .expect("a described release rolls back");

    let [(_, message)] = desired.messages().try_into().expect("one write");
    assert_eq!(
        message,
        format!(
            "Roll saas-fabric in lucentroot back to 0.3.0-preview.1\n\nBuilt from {COMMIT}.\nComponent descriptor {}.",
            descriptor_digest("0.3.0-preview.1")
        )
    );
}

#[tokio::test]
async fn the_console_is_told_images_and_every_diagnostic_in_its_own_list() {
    let registry = Arc::new(FakeRegistry::default());
    registry.publish_images("0.3.0-preview.3", COMMIT);
    registry.publish_images("0.3.0-preview.4", COMMIT);
    registry.describe("0.3.0-preview.4", Attached::Several { digests: Vec::new() });
    let desired = Arc::new(Written::at("0.3.0-preview.2"));

    let status = service(&registry, &desired)
        .status("lucentroot", "saas-fabric")
        .await
        .expect("status reads");

    // A rollback restores the same exact bytes, so the kind is images.
    assert_eq!(status.artifact, ArtifactKind::Oci);
    assert_eq!(status.newer, None);
    assert_eq!(status.diagnostics.undescribed, vec![version("0.3.0-preview.3")]);
    assert_eq!(
        status.diagnostics.invalid,
        vec![InvalidVersion {
            version: version("0.3.0-preview.4"),
            reason: InvalidReason::Several,
        }]
    );
    assert!(desired.messages().is_empty(), "a read never writes");
}
