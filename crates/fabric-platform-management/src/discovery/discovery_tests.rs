//! What discovery does with a registry mid-publish.

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::discover;
use crate::{Channel, Registry, RegistryError, Resolved, Version};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const CONTROL_PLANE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane";
const CONSOLE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane-ui";

/// A registry whose contents a test can change between passes.
#[derive(Default)]
struct FakeRegistry {
    /// `(repository, tag)` to what it resolves to.
    published: Mutex<BTreeMap<(String, String), Resolved>>,
}

impl FakeRegistry {
    /// Publishes one image of one version.
    fn publish(&self, repository: &str, tag: &str, revision: &str) {
        self.published
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                (repository.to_owned(), tag.to_owned()),
                Resolved {
                    digest: format!(
                        "sha256:{}",
                        format!("{repository}{tag}").len().to_string().repeat(8)
                    ),
                    revision: Some(revision.to_owned()),
                },
            );
    }

    /// Publishes every image of a version, from one commit.
    fn publish_all(&self, tag: &str, revision: &str) {
        for repository in [RUNTIME, CONTROL_PLANE, CONSOLE] {
            self.publish(repository, tag, revision);
        }
    }
}

#[async_trait::async_trait]
impl Registry for FakeRegistry {
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        Ok(self
            .published
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .filter(|(published, _)| published == repository)
            .map(|(_, tag)| tag.clone())
            .collect())
    }

    async fn resolve(&self, repository: &str, tag: &str) -> Result<Option<Resolved>, RegistryError> {
        Ok(self
            .published
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&(repository.to_owned(), tag.to_owned()))
            .cloned())
    }
}

fn roles() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("console".to_owned(), CONSOLE.to_owned()),
        ("controlPlane".to_owned(), CONTROL_PLANE.to_owned()),
        ("runtime".to_owned(), RUNTIME.to_owned()),
    ])
}

fn version(text: &str) -> Version {
    Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
}

/// A pass over the preview channel of the 0.3.0 line, above `floor`.
async fn pass(registry: &FakeRegistry, floor: &str) -> crate::Discovery {
    discover(
        registry,
        &roles(),
        Channel::Preview,
        Some(&version("0.3.0")),
        &version(floor),
    )
    .await
    .expect("the fake registry never fails")
}

#[tokio::test]
async fn an_incomplete_version_becomes_available_on_a_later_pass() {
    // The property the whole design rests on. The three images are published
    // by parallel jobs, so a version existing in two repositories and not the
    // third is an ordinary window -- and nothing may remember it as rejected.
    let registry = FakeRegistry::default();
    registry.publish_all("0.3.0-preview.2", "aaaa");
    registry.publish(RUNTIME, "0.3.0-preview.3", "bbbb");
    registry.publish(CONTROL_PLANE, "0.3.0-preview.3", "bbbb");

    let first = pass(&registry, "0.3.0-preview.1").await;

    assert_eq!(
        first.available.as_ref().map(|unit| unit.version.as_str()),
        Some("0.3.0-preview.2"),
        "the newest *complete* version is the answer"
    );
    assert_eq!(first.not_yet, vec![version("0.3.0-preview.3")]);

    // The console's job finishes. No restart, no cache expiry, no new object:
    // the same registry, asked again.
    registry.publish(CONSOLE, "0.3.0-preview.3", "bbbb");

    let second = pass(&registry, "0.3.0-preview.1").await;

    assert_eq!(
        second.available.as_ref().map(|unit| unit.version.as_str()),
        Some("0.3.0-preview.3"),
        "an incomplete version must be reconsidered, not remembered as refused"
    );
    assert!(second.not_yet.is_empty());
}

#[tokio::test]
async fn every_image_must_come_from_the_same_commit() {
    let registry = FakeRegistry::default();
    registry.publish_all("0.3.0-preview.2", "aaaa");
    registry.publish(RUNTIME, "0.3.0-preview.3", "bbbb");
    registry.publish(CONTROL_PLANE, "0.3.0-preview.3", "bbbb");
    // Rebuilt, from somewhere else.
    registry.publish(CONSOLE, "0.3.0-preview.3", "cccc");

    let found = pass(&registry, "0.3.0-preview.1").await;

    assert_eq!(found.incoherent, vec![version("0.3.0-preview.3")]);
    assert!(found.not_yet.is_empty(), "this is not a publishing window");
    assert_eq!(
        found.available.map(|unit| unit.version.as_str().to_owned()),
        Some("0.3.0-preview.2".to_owned())
    );
}

#[tokio::test]
async fn an_image_with_no_provenance_is_not_promoted() {
    let registry = FakeRegistry::default();
    registry.publish_all("0.3.0-preview.2", "aaaa");
    registry.publish(RUNTIME, "0.3.0-preview.3", "bbbb");
    registry.publish(CONTROL_PLANE, "0.3.0-preview.3", "bbbb");
    registry
        .published
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            (CONSOLE.to_owned(), "0.3.0-preview.3".to_owned()),
            Resolved {
                digest: "sha256:unlabelled".to_owned(),
                revision: None,
            },
        );

    let found = pass(&registry, "0.3.0-preview.1").await;

    // Incomplete rather than incoherent: a missing label is indistinguishable
    // from a push still in flight, and waiting is the cheaper mistake.
    assert_eq!(found.not_yet, vec![version("0.3.0-preview.3")]);
    assert_eq!(
        found.available.map(|unit| unit.version.as_str().to_owned()),
        Some("0.3.0-preview.2".to_owned())
    );
}

#[tokio::test]
async fn selection_never_moves_backwards() {
    let registry = FakeRegistry::default();
    for tag in [
        "0.3.0-preview.1",
        "0.3.0-preview.2",
        "0.3.0-preview.9",
        "0.3.0-preview.10",
    ] {
        registry.publish_all(tag, "aaaa");
    }

    let found = pass(&registry, "0.3.0-preview.9").await;

    // `preview.10` and not `preview.9`, and certainly not `preview.2`. String
    // order would have made 9 the newest and this test would fail.
    assert_eq!(
        found.available.map(|unit| unit.version.as_str().to_owned()),
        Some("0.3.0-preview.10".to_owned())
    );

    let settled = pass(&registry, "0.3.0-preview.10").await;
    assert!(settled.available.is_none(), "nothing newer exists");
}

#[tokio::test]
async fn a_channel_and_a_series_both_bound_what_is_eligible() {
    let registry = FakeRegistry::default();
    registry.publish_all("0.3.0-preview.2", "aaaa");
    registry.publish_all("0.3.0", "aaaa");
    registry.publish_all("0.4.0-preview.1", "aaaa");

    let found = pass(&registry, "0.3.0-preview.1").await;

    // `0.3.0` is stable, and `0.4.0-preview.1` is another line. An automatic
    // preview policy must not walk an environment onto either.
    assert_eq!(
        found.available.map(|unit| unit.version.as_str().to_owned()),
        Some("0.3.0-preview.2".to_owned())
    );
}

#[tokio::test]
async fn a_complete_unit_carries_every_image_and_the_commit_they_share() {
    let registry = FakeRegistry::default();
    registry.publish_all("0.3.0-preview.2", "5707f5e");

    let unit = pass(&registry, "0.3.0-preview.1")
        .await
        .available
        .expect("a complete version");

    assert_eq!(unit.source_revision, "5707f5e");
    assert_eq!(unit.images.len(), 3);
    assert_eq!(unit.images["runtime"].repository, RUNTIME);
    assert!(unit
        .images
        .values()
        .all(|image| image.digest.starts_with("sha256:")));
}
