//! What this adapter reads off the wire, and what it does with what it finds.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;

use fabric_platform_management::{discover, Channel, Provenance, Registry, RegistryError, Version};
use fabric_registry::OciRegistry;

mod support;

use support::{FakeRegistry, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const CONTROL_PLANE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane";
const CONSOLE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane-ui";

fn registry(fake: &FakeRegistry) -> OciRegistry {
    OciRegistry::new(&fake.base_url, HOST, 5).unwrap()
}

#[tokio::test]
async fn a_tag_resolves_to_its_digest_and_its_source_commit() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0-preview.2", "5707f5e");

    let resolved = registry(&fake)
        .resolve(RUNTIME, "0.3.0-preview.2")
        .await
        .unwrap()
        .expect("a published tag");

    assert_eq!(resolved.digest, fake.digest_for(RUNTIME, "0.3.0-preview.2"));
    assert_eq!(resolved.provenance, Provenance::Agreed("5707f5e".to_owned()));
}

#[tokio::test]
async fn a_tag_that_is_not_published_is_an_answer_and_not_a_failure() {
    // The property the whole design rests on. A version present in two
    // repositories and not the third is a publishing window, and an adapter
    // reporting that as an error would turn every such window into a failed
    // discovery pass.
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0-preview.2", "5707f5e");

    let missing = registry(&fake).resolve(RUNTIME, "0.3.0-preview.3").await.unwrap();

    assert!(missing.is_none());
}

#[tokio::test]
async fn a_repository_may_be_named_with_or_without_the_registry_host() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let registry = registry(&fake);

    let with = registry.resolve(RUNTIME, "0.3.0").await.unwrap();
    let without = registry
        .resolve("fieldstatenz/saas-fabric", "0.3.0")
        .await
        .unwrap();

    assert!(with.is_some());
    assert_eq!(with, without, "a manifest and this adapter must not disagree");
}

#[tokio::test]
async fn a_multi_architecture_image_pins_the_index_and_every_child_must_agree() {
    let fake = FakeRegistry::start().await;
    fake.publish_index(RUNTIME, "0.4.0", &[("amd64", "deadbee"), ("arm64", "deadbee")]);

    let resolved = registry(&fake).resolve(RUNTIME, "0.4.0").await.unwrap().unwrap();

    // The index's digest, not a platform's: that is what a Deployment should
    // reference, so the node picks its own architecture.
    assert_eq!(resolved.digest, fake.digest_for(RUNTIME, "0.4.0"));
    assert_eq!(resolved.provenance, Provenance::Agreed("deadbee".to_owned()));
}

#[tokio::test]
async fn an_index_whose_children_disagree_is_not_a_release_artifact() {
    // Reading amd64's label and stopping would call this provenanced, because
    // amd64 is what LucentRoot happens to run. "The architecture we run today"
    // is not a fact about the image.
    let fake = FakeRegistry::start().await;
    fake.publish_index(RUNTIME, "0.4.0", &[("amd64", "deadbee"), ("arm64", "c0ffee")]);

    let resolved = registry(&fake).resolve(RUNTIME, "0.4.0").await.unwrap().unwrap();

    assert_eq!(resolved.provenance, Provenance::Disagreed);
}

#[tokio::test]
async fn an_attestation_manifest_is_not_mistaken_for_an_image() {
    // Build systems put attestations in the same index under unknown/unknown.
    // They carry no revision, and inspecting them would report every
    // multi-architecture image as unprovenanced.
    let fake = FakeRegistry::start().await;
    fake.publish_index(RUNTIME, "0.4.0", &[("amd64", "deadbee")]);

    let resolved = registry(&fake).resolve(RUNTIME, "0.4.0").await.unwrap().unwrap();

    assert_eq!(resolved.provenance, Provenance::Agreed("deadbee".to_owned()));
}

#[tokio::test]
async fn an_index_with_nothing_deployable_in_it_proves_nothing() {
    // Zero deployable children must not agree with themselves. There is no
    // artifact here to prove the provenance *of*, so this is Absent -- the
    // answer that means "not yet", not the one that means "built twice".
    let fake = FakeRegistry::start().await;
    fake.publish_index_with_no_image(RUNTIME, "0.4.0");

    let resolved = registry(&fake).resolve(RUNTIME, "0.4.0").await.unwrap().unwrap();

    assert_eq!(resolved.provenance, Provenance::Absent);
}

#[tokio::test]
async fn an_image_with_no_labels_reports_no_revision() {
    let fake = FakeRegistry::start().await;
    fake.publish_unlabelled(RUNTIME, "0.3.0-preview.9");

    let resolved = registry(&fake)
        .resolve(RUNTIME, "0.3.0-preview.9")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(resolved.provenance, Provenance::Absent);
}

#[tokio::test]
async fn every_page_of_tags_is_followed() {
    let fake = FakeRegistry::start().await;
    for index in 1..=5 {
        fake.publish(RUNTIME, &format!("0.3.0-preview.{index}"), "abc");
    }
    fake.paginate();

    let tags = registry(&fake).tags(RUNTIME).await.unwrap();

    // A truncated listing looks exactly like a component whose newer versions
    // do not exist, and discovery would quietly stop advancing.
    assert_eq!(tags.len(), 5, "got {tags:?}");
}

#[tokio::test]
async fn a_repository_that_has_never_been_published_lists_nothing() {
    let fake = FakeRegistry::start().await;

    assert!(registry(&fake).tags(RUNTIME).await.unwrap().is_empty());
}

#[tokio::test]
async fn an_expired_token_is_replaced_rather_than_failing_the_pass() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let registry = registry(&fake);

    registry.resolve(RUNTIME, "0.3.0").await.unwrap().unwrap();
    let after_first = fake.mints();

    // The token it is holding stops working, as one that has aged out does.
    fake.expire_the_current_token();

    let resolved = registry.resolve(RUNTIME, "0.3.0").await.unwrap();

    assert!(resolved.is_some(), "a 401 must mint a new token and retry");
    assert_eq!(fake.mints(), after_first + 1);
}

#[tokio::test]
async fn a_rate_limit_leaves_availability_stale_rather_than_wrong() {
    let fake = FakeRegistry::start().await;
    fake.tags_answer(429);

    let failure = registry(&fake).tags(RUNTIME).await.expect_err("429 is a failure");

    // Unavailable, not Refused: the difference is whether a caller should try
    // again, and a rate limit is the case where it should.
    assert!(
        matches!(failure, RegistryError::Unavailable { .. }),
        "{failure:?}"
    );
}

#[tokio::test]
async fn discovery_runs_end_to_end_over_the_wire() {
    // The rules are tested against a three-line fake in
    // fabric-platform-management. This is the same property through the real
    // adapter and a real socket: what a registry mid-publish actually does to
    // a discovery pass.
    let fake = FakeRegistry::start().await;
    let roles = BTreeMap::from([
        ("console".to_owned(), CONSOLE.to_owned()),
        ("controlPlane".to_owned(), CONTROL_PLANE.to_owned()),
        ("runtime".to_owned(), RUNTIME.to_owned()),
    ]);

    for repository in [RUNTIME, CONTROL_PLANE, CONSOLE] {
        fake.publish(repository, "0.3.0-preview.2", "aaaa");
    }
    // `.3` is still publishing: the console's job has not finished.
    fake.publish(RUNTIME, "0.3.0-preview.3", "bbbb");
    fake.publish(CONTROL_PLANE, "0.3.0-preview.3", "bbbb");

    let registry = registry(&fake);
    let floor = Version::parse("0.3.0-preview.1").unwrap();
    let series = Version::parse("0.3.0").unwrap();

    let first = discover(&registry, &roles, Channel::Preview, Some(&series), &floor)
        .await
        .unwrap();

    assert_eq!(
        first.newer.as_ref().map(|unit| unit.version.as_str()),
        Some("0.3.0-preview.2")
    );
    assert_eq!(first.not_yet, vec![Version::parse("0.3.0-preview.3").unwrap()]);

    fake.publish(CONSOLE, "0.3.0-preview.3", "bbbb");

    let second = discover(&registry, &roles, Channel::Preview, Some(&series), &floor)
        .await
        .unwrap();

    assert_eq!(
        second.newer.as_ref().map(|unit| unit.version.as_str()),
        Some("0.3.0-preview.3"),
        "the same adapter, asked again, must see the completed release"
    );
    assert!(second.not_yet.is_empty());

    let unit = second.newer.unwrap();
    assert_eq!(unit.source_revision, "bbbb");
    assert_eq!(
        unit.images["console"].digest,
        fake.digest_for(CONSOLE, "0.3.0-preview.3")
    );
}
