//! An image's revisions are its config's labels and its manifest's
//! annotations, and for an index the index's annotations too.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_platform_management::{Provenance, Registry};
use fabric_registry::OciRegistry;
use support::{FakeRegistry, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";

async fn provenance_of_image(label: Option<&str>, annotation: Option<&str>) -> Provenance {
    let fake = FakeRegistry::start().await;
    fake.publish_annotated(RUNTIME, "0.3.0", label, annotation);
    resolve(&fake).await
}

async fn provenance_of_index(
    annotation: Option<&str>,
    children: &[(&str, Option<&str>, Option<&str>)],
) -> Provenance {
    let fake = FakeRegistry::start().await;
    fake.publish_annotated_index(RUNTIME, "0.3.0", annotation, children);
    resolve(&fake).await
}

async fn resolve(fake: &FakeRegistry) -> Provenance {
    OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5)
        .unwrap()
        .resolve(RUNTIME, "0.3.0")
        .await
        .unwrap()
        .expect("published")
        .provenance
}

fn agreed(revision: &str) -> Provenance {
    Provenance::Agreed(revision.to_owned())
}

#[tokio::test]
async fn an_image_may_carry_its_revision_as_a_label_an_annotation_or_both() {
    assert_eq!(provenance_of_image(Some("abc"), None).await, agreed("abc"));
    assert_eq!(provenance_of_image(None, Some("abc")).await, agreed("abc"));
    assert_eq!(provenance_of_image(Some("abc"), Some("abc")).await, agreed("abc"));
}

#[tokio::test]
async fn a_label_and_an_annotation_that_differ_disagree() {
    assert_eq!(
        provenance_of_image(Some("abc"), Some("def")).await,
        Provenance::Disagreed
    );
}

#[tokio::test]
async fn an_image_with_neither_is_absent() {
    assert_eq!(provenance_of_image(None, None).await, Provenance::Absent);
}

#[tokio::test]
async fn an_index_annotation_counts_for_every_deployable_child() {
    let children = [("amd64", None, None), ("arm64", None, Some("abc"))];

    assert_eq!(provenance_of_index(Some("abc"), &children).await, agreed("abc"));
}

#[tokio::test]
async fn an_index_annotation_that_differs_from_a_child_disagrees() {
    let children = [("amd64", Some("abc"), None), ("arm64", Some("abc"), None)];

    assert_eq!(
        provenance_of_index(Some("def"), &children).await,
        Provenance::Disagreed
    );
}

#[tokio::test]
async fn a_child_with_no_revision_anywhere_makes_the_index_absent() {
    let children = [("amd64", Some("abc"), None), ("arm64", None, None)];

    assert_eq!(provenance_of_index(None, &children).await, Provenance::Absent);
}

#[tokio::test]
async fn children_disagreeing_is_disagreement_even_beside_a_child_with_none() {
    let children = [
        ("amd64", None, None),
        ("arm64", Some("abc"), None),
        ("s390x", None, Some("def")),
    ];

    assert_eq!(provenance_of_index(None, &children).await, Provenance::Disagreed);
}
