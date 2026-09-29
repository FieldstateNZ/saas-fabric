//! Discovering, listing and resolving a described component: the one rule,
//! asked of each version the primary repository tags.

use super::described::fake_registry_tests::{
    descriptor_digest, image_digest, pins, FakeRegistry, CONSOLE, CONTROL_PLANE, RUNTIME,
};
use super::{described_history, discover_described, resolve_described, InvalidReason, InvalidVersion};
use crate::{Attached, Channel, Discovery, History, Release, Version};

fn version(text: &str) -> Version {
    Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
}

async fn discover(registry: &FakeRegistry, floor: &str) -> Discovery {
    discover_described(
        registry,
        "runtime",
        &pins(),
        Channel::Preview,
        Some(&version("0.3.0")),
        &version(floor),
    )
    .await
    .expect("the fake registry answers")
}

async fn listing(registry: &FakeRegistry, floor: &str) -> History {
    described_history(
        registry,
        "runtime",
        &pins(),
        Channel::Preview,
        Some(&version("0.3.0")),
        &version(floor),
    )
    .await
    .expect("the fake registry answers")
}

async fn one(registry: &FakeRegistry, floor: &str, wanted: &str) -> Option<Release> {
    resolve_described(
        registry,
        "runtime",
        &pins(),
        Channel::Preview,
        Some(&version("0.3.0")),
        &version(floor),
        wanted,
    )
    .await
    .expect("the fake registry answers")
}

fn preview(number: u32) -> String {
    format!("0.3.0-preview.{number}")
}

#[tokio::test]
async fn the_newest_complete_version_is_newer_and_every_other_is_reported_as_what_it_is() {
    let registry = FakeRegistry::default();
    registry.publish(&preview(2), "aaaa");
    // Undescribed: every image, and nothing attached.
    registry.publish_images(&preview(3), "bbbb");
    // Incoherent: the component descriptor from another commit.
    registry.publish_images(&preview(4), "cccc");
    registry.describe(
        &preview(4),
        super::described::fake_registry_tests::attached(
            &preview(4),
            super::described::fake_registry_tests::spec(&preview(4)),
            "dddd",
        ),
    );
    // Invalid: several attached.
    registry.publish_images(&preview(5), "eeee");
    registry.describe(&preview(5), Attached::Several { digests: Vec::new() });

    let found = discover(&registry, &preview(1)).await;

    let Some(Release::Described {
        unit,
        primary,
        descriptor,
    }) = &found.newer
    else {
        panic!("preview.2 is a described release: {found:?}");
    };
    assert_eq!(unit.version.as_str(), preview(2));
    assert_eq!(primary, "runtime");
    assert_eq!(descriptor, &descriptor_digest(&preview(2)));
    assert_eq!(found.undescribed, vec![version(&preview(3))]);
    assert_eq!(found.incoherent, vec![version(&preview(4))]);
    assert_eq!(
        found.invalid,
        vec![InvalidVersion {
            version: version(&preview(5)),
            reason: InvalidReason::Several,
        }]
    );
    assert!(
        found.not_yet.is_empty(),
        "a described component is never 'publishing'"
    );
}

#[tokio::test]
async fn an_undescribed_version_becomes_newer_once_its_descriptor_is_attached() {
    // Nothing is remembered: the same registry, asked again.
    let registry = FakeRegistry::default();
    registry.publish_images(&preview(2), "aaaa");

    let first = discover(&registry, &preview(1)).await;
    assert!(first.newer.is_none());
    assert_eq!(first.undescribed, vec![version(&preview(2))]);

    registry.publish(&preview(2), "aaaa");

    let second = discover(&registry, &preview(1)).await;
    assert_eq!(
        second.newer.as_ref().map(|release| release.version().as_str()),
        Some(preview(2).as_str())
    );
    assert!(second.undescribed.is_empty());
}

#[tokio::test]
async fn only_the_primary_repository_says_which_versions_exist() {
    // A version the console repository carries and the runtime does not is
    // not a candidate: the component descriptor lives beside the primary.
    let registry = FakeRegistry::default();
    registry.image(
        CONSOLE,
        &preview(2),
        &image_digest(&preview(2), "console"),
        crate::Provenance::Agreed("aaaa".to_owned()),
    );
    registry.image(
        CONTROL_PLANE,
        &preview(2),
        &image_digest(&preview(2), "controlPlane"),
        crate::Provenance::Agreed("aaaa".to_owned()),
    );

    assert_eq!(discover(&registry, &preview(1)).await, Discovery::default());
}

#[tokio::test]
async fn a_tag_gone_since_the_listing_is_skipped_rather_than_reported() {
    let registry = FakeRegistry::default();
    registry.list_only(RUNTIME, &preview(3));
    registry.publish(&preview(2), "aaaa");

    let found = discover(&registry, &preview(1)).await;

    assert_eq!(
        found.newer.as_ref().map(|release| release.version().as_str()),
        Some(preview(2).as_str())
    );
    assert!(found.undescribed.is_empty() && found.invalid.is_empty() && found.incoherent.is_empty());
}

#[tokio::test]
async fn nothing_at_or_below_the_floor_or_outside_the_channel_or_series_is_considered() {
    let registry = FakeRegistry::default();
    registry.publish(&preview(1), "aaaa");
    registry.publish("0.3.0", "bbbb");
    registry.publish("0.4.0-preview.1", "cccc");

    assert_eq!(discover(&registry, &preview(1)).await, Discovery::default());
}

#[tokio::test]
async fn a_primary_that_is_not_pinned_finds_nothing() {
    let registry = FakeRegistry::default();
    registry.publish(&preview(2), "aaaa");

    let found = discover_described(
        &registry,
        "sidecar",
        &pins(),
        Channel::Preview,
        None,
        &version(&preview(1)),
    )
    .await
    .unwrap();

    assert_eq!(found, Discovery::default());
}

#[tokio::test]
async fn the_listing_offers_only_complete_releases_below_and_says_when_there_are_more() {
    let registry = FakeRegistry::default();
    for number in 1..=7 {
        registry.publish(&preview(number), "aaaa");
    }
    // Among the five examined, one is undescribed and dropped.
    registry.publish_images(&preview(6), "aaaa");
    registry.describe(&preview(6), Attached::Nothing);

    let found = listing(&registry, &preview(8)).await;

    let offered: Vec<&str> = found
        .releases
        .iter()
        .map(|release| release.version().as_str())
        .collect();
    assert_eq!(
        offered,
        [
            "0.3.0-preview.7",
            "0.3.0-preview.5",
            "0.3.0-preview.4",
            "0.3.0-preview.3"
        ]
    );
    assert!(found.more, "preview.2 and preview.1 were not examined");
    assert!(found
        .releases
        .iter()
        .all(|release| matches!(release, Release::Described { .. })));
}

#[tokio::test]
async fn one_named_version_is_resolved_now_by_the_same_rule() {
    let registry = FakeRegistry::default();
    registry.publish(&preview(1), "aaaa");
    registry.publish_images(&preview(2), "bbbb");
    registry.publish(&preview(4), "cccc");

    assert!(matches!(
        one(&registry, &preview(3), &preview(1)).await,
        Some(Release::Described { .. })
    ));
    assert_eq!(
        one(&registry, &preview(3), &preview(2)).await,
        None,
        "undescribed"
    );
    assert_eq!(one(&registry, &preview(3), &preview(4)).await, None, "not below");
    assert_eq!(one(&registry, &preview(3), "not-a-version").await, None);
}
