//! The one rule, answer by answer and reason by reason, and which failure
//! outranks which.

use std::collections::BTreeMap;

use fabric_component::{Digest, Role};

use super::fake_registry_tests::{
    attached, descriptor_digest, digest, image_digest, pins, spec, FakeRegistry, CONSOLE, CONTROL_PLANE,
    RUNTIME,
};
use super::{evaluate, Evaluation, Expectation, InvalidReason, RevisionOf};
use crate::{Attached, AttachedDescriptor, Provenance, RegistryError, Unusable};

const VERSION: &str = "0.3.0-preview.3";
const COMMIT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// Evaluates `VERSION` with the pins every test environment has.
async fn pinned(registry: &FakeRegistry) -> Evaluation {
    pinned_as(registry, "runtime", &pins()).await
}

async fn pinned_as(
    registry: &FakeRegistry,
    primary: &str,
    repositories: &BTreeMap<String, String>,
) -> Evaluation {
    evaluate(
        registry,
        RUNTIME,
        VERSION,
        Expectation::Pinned {
            primary,
            repositories,
        },
    )
    .await
    .expect("the fake registry answers")
}

fn invalid(reason: InvalidReason) -> Evaluation {
    Evaluation::Invalid(reason)
}

/// A release published whole, from one commit.
fn published() -> FakeRegistry {
    let registry = FakeRegistry::default();
    registry.publish(VERSION, COMMIT);
    registry
}

/// The one attached component descriptor, changed by `change`.
fn with_attached(registry: &FakeRegistry, change: impl FnOnce(&mut AttachedDescriptor)) {
    let Attached::One(mut one) = attached(VERSION, spec(VERSION), COMMIT) else {
        unreachable!("attached always builds one");
    };
    change(&mut one);
    registry.describe(VERSION, Attached::One(one));
}

#[tokio::test]
async fn a_whole_release_is_complete_with_every_digest_it_computed() {
    let Evaluation::Complete(found) = pinned(&published()).await else {
        panic!("a whole release is a release unit");
    };

    assert_eq!(found.unit.version.as_str(), VERSION);
    assert_eq!(found.unit.source_revision, COMMIT);
    assert_eq!(found.primary, "runtime");
    assert_eq!(found.descriptor_digest, descriptor_digest(VERSION));
    assert_eq!(found.descriptor.spec().version.as_str(), VERSION);
    assert_eq!(found.unit.images.len(), 3);
    for (role, repository) in [
        ("console", CONSOLE),
        ("controlPlane", CONTROL_PLANE),
        ("runtime", RUNTIME),
    ] {
        let image = &found.unit.images[role];
        assert_eq!(image.repository, repository);
        assert_eq!(image.digest, image_digest(VERSION, role));
    }
}

#[tokio::test]
async fn a_version_with_no_tag_is_not_tagged() {
    assert_eq!(pinned(&FakeRegistry::default()).await, Evaluation::NotTagged);
}

#[tokio::test]
async fn a_tag_with_nothing_attached_is_undescribed() {
    let registry = FakeRegistry::default();
    registry.publish_images(VERSION, COMMIT);

    assert_eq!(pinned(&registry).await, Evaluation::Undescribed);
}

#[tokio::test]
async fn several_attached_are_refused_rather_than_chosen_between() {
    let registry = published();
    registry.describe(
        VERSION,
        Attached::Several {
            digests: vec![digest("one"), digest("two")],
        },
    );

    assert_eq!(pinned(&registry).await, invalid(InvalidReason::Several));
}

#[tokio::test]
async fn anything_the_adapter_could_not_use_is_unreadable() {
    for reason in [Unusable::NotAnIndex, Unusable::OtherSubject, Unusable::Malformed] {
        let registry = published();
        registry.describe(VERSION, Attached::Unusable { reason });

        assert_eq!(
            pinned(&registry).await,
            invalid(InvalidReason::Unreadable),
            "{reason:?}"
        );
    }
}

#[tokio::test]
async fn a_document_that_is_not_a_component_descriptor_is_unreadable() {
    let registry = published();
    with_attached(&registry, |one| one.document = b"{}".to_vec());

    assert_eq!(pinned(&registry).await, invalid(InvalidReason::Unreadable));
}

#[tokio::test]
async fn an_image_on_another_registry_is_named_as_such_not_as_unreadable() {
    // Fabric read the document; it names a registry the primary is not on.
    let registry = published();
    with_attached(&registry, |one| {
        let text = String::from_utf8(one.document.clone()).unwrap();
        let moved = CONSOLE.replacen("ghcr.io", "docker.io", 1);
        one.document = text.replacen(CONSOLE, &moved, 1).into_bytes();
    });

    assert_eq!(pinned(&registry).await, invalid(InvalidReason::OtherRegistry));
}

#[tokio::test]
async fn a_version_this_build_does_not_read_is_named_and_never_undescribed() {
    let registry = published();
    with_attached(&registry, |one| {
        one.artifact_type = "application/vnd.saas-fabric.component.v2".to_owned();
    });

    assert_eq!(
        pinned(&registry).await,
        invalid(InvalidReason::UnsupportedVersion {
            found: "v2".to_owned()
        })
    );
}

#[tokio::test]
async fn a_document_naming_another_version_is_the_wrong_version() {
    // The tag says preview.3 and the document preview.4: one digest, two
    // versions, and neither is believed.
    let registry = published();
    let mut other = spec("0.3.0-preview.4");
    other.images = spec(VERSION).images;
    registry.describe(VERSION, attached(VERSION, other, COMMIT));

    assert_eq!(pinned(&registry).await, invalid(InvalidReason::WrongVersion));
}

#[tokio::test]
async fn a_version_annotation_naming_another_version_is_the_wrong_version() {
    let registry = published();
    with_attached(&registry, |one| one.version = Some("0.3.0-preview.4".to_owned()));

    assert_eq!(pinned(&registry).await, invalid(InvalidReason::WrongVersion));
}

#[tokio::test]
async fn no_version_annotation_is_not_a_wrong_one() {
    let registry = published();
    with_attached(&registry, |one| one.version = None);

    assert!(matches!(pinned(&registry).await, Evaluation::Complete(_)));
}

#[tokio::test]
async fn a_document_that_does_not_name_the_image_it_is_attached_to_is_refused() {
    let registry = published();
    let mut moved = spec(VERSION);
    moved
        .images
        .get_mut(&Role::try_new("runtime").unwrap())
        .unwrap()
        .digest = Digest::try_new(digest("elsewhere")).unwrap();
    registry.describe(VERSION, attached(VERSION, moved, COMMIT));

    assert_eq!(pinned(&registry).await, invalid(InvalidReason::PrimaryNotNamed));
}

#[tokio::test]
async fn roles_repositories_or_a_primary_other_than_the_pins_are_not_pinned() {
    let registry = published();

    let mut fewer = pins();
    fewer.remove("console");
    assert_eq!(
        pinned_as(&registry, "runtime", &fewer).await,
        invalid(InvalidReason::NotPinned)
    );

    let mut elsewhere = pins();
    elsewhere.insert("console".to_owned(), "ghcr.io/fieldstatenz/other".to_owned());
    assert_eq!(
        pinned_as(&registry, "runtime", &elsewhere).await,
        invalid(InvalidReason::NotPinned)
    );

    assert_eq!(
        pinned_as(&registry, "controlPlane", &pins()).await,
        invalid(InvalidReason::NotPinned)
    );
}

#[tokio::test]
async fn a_repository_nobody_registered_is_named() {
    let registry = published();
    let registered = |repository: &str| repository == RUNTIME;

    let answer = evaluate(&registry, RUNTIME, VERSION, Expectation::Registered(&registered))
        .await
        .unwrap();

    // Role order: console sorts first.
    assert_eq!(
        answer,
        invalid(InvalidReason::NotRegistered {
            repository: CONSOLE.to_owned()
        })
    );

    let everything = |_: &str| true;
    let answer = evaluate(&registry, RUNTIME, VERSION, Expectation::Registered(&everything))
        .await
        .unwrap();
    assert!(matches!(answer, Evaluation::Complete(_)));
}

#[tokio::test]
async fn an_image_that_does_not_exist_at_its_digest_is_missing() {
    let registry = published();
    registry.delete(CONSOLE, &image_digest(VERSION, "console"));

    assert_eq!(
        pinned(&registry).await,
        invalid(InvalidReason::MissingImage {
            role: "console".to_owned()
        })
    );
}

#[tokio::test]
async fn an_image_with_no_single_revision_is_named() {
    for provenance in [
        Provenance::Absent,
        Provenance::Disagreed,
        Provenance::Agreed(String::new()),
        // Text a catalogue could not record is not a revision either.
        Provenance::Agreed("c".repeat(257)),
        Provenance::Agreed(format!("{COMMIT}\n")),
    ] {
        let registry = published();
        registry.untagged(
            CONTROL_PLANE,
            &image_digest(VERSION, "controlPlane"),
            provenance.clone(),
        );

        assert_eq!(
            pinned(&registry).await,
            invalid(InvalidReason::NoSingleRevision {
                of: RevisionOf::Image {
                    role: "controlPlane".to_owned()
                }
            }),
            "{provenance:?}"
        );
    }
}

#[tokio::test]
async fn the_primary_with_no_single_revision_is_named_too() {
    let registry = published();
    registry.untagged(RUNTIME, &image_digest(VERSION, "runtime"), Provenance::Disagreed);

    assert_eq!(
        pinned(&registry).await,
        invalid(InvalidReason::NoSingleRevision {
            of: RevisionOf::Image {
                role: "runtime".to_owned()
            }
        })
    );
}

#[tokio::test]
async fn another_image_without_the_version_tag_is_incoherent() {
    let registry = published();
    registry.untag(CONSOLE, VERSION);

    assert_eq!(pinned(&registry).await, Evaluation::Incoherent);
}

#[tokio::test]
async fn another_image_whose_version_tag_moved_is_incoherent() {
    // Rebuilt and pushed again under the same version: the tag now points at
    // bytes the component descriptor does not name.
    let registry = published();
    let rebuilt = digest("rebuilt console");
    registry.image(CONSOLE, VERSION, &rebuilt, Provenance::Agreed(COMMIT.to_owned()));

    assert_eq!(pinned(&registry).await, Evaluation::Incoherent);
}

#[tokio::test]
async fn a_component_descriptor_with_no_revision_is_named() {
    for revision in [
        None,
        Some(String::new()),
        Some("c".repeat(257)),
        Some(format!("{COMMIT}\n")),
    ] {
        let registry = published();
        with_attached(&registry, |one| one.revision.clone_from(&revision));

        assert_eq!(
            pinned(&registry).await,
            invalid(InvalidReason::NoSingleRevision {
                of: RevisionOf::ComponentDescriptor
            }),
            "{revision:?}"
        );
    }
}

#[tokio::test]
async fn a_component_descriptor_from_another_commit_is_incoherent() {
    let registry = published();
    with_attached(&registry, |one| one.revision = Some("c".repeat(40)));

    assert_eq!(pinned(&registry).await, Evaluation::Incoherent);
}

#[tokio::test]
async fn images_from_different_commits_are_incoherent() {
    let registry = published();
    registry.untagged(
        CONSOLE,
        &image_digest(VERSION, "console"),
        Provenance::Agreed("c".repeat(40)),
    );

    assert_eq!(pinned(&registry).await, Evaluation::Incoherent);
}

#[tokio::test]
async fn a_missing_image_outranks_a_disagreeing_commit() {
    let registry = published();
    registry.untagged(
        CONSOLE,
        &image_digest(VERSION, "console"),
        Provenance::Agreed("c".repeat(40)),
    );
    registry.delete(CONTROL_PLANE, &image_digest(VERSION, "controlPlane"));

    assert_eq!(
        pinned(&registry).await,
        invalid(InvalidReason::MissingImage {
            role: "controlPlane".to_owned()
        })
    );
}

#[tokio::test]
async fn a_missing_image_outranks_one_with_no_revision_that_sorts_first() {
    let registry = published();
    registry.untagged(CONSOLE, &image_digest(VERSION, "console"), Provenance::Absent);
    registry.delete(CONTROL_PLANE, &image_digest(VERSION, "controlPlane"));

    assert_eq!(
        pinned(&registry).await,
        invalid(InvalidReason::MissingImage {
            role: "controlPlane".to_owned()
        })
    );
}

#[tokio::test]
async fn a_moved_version_tag_outranks_an_image_with_no_revision() {
    // Step 4 before step 5 (ADR 0026 section 3): one version built twice is
    // the answer, whatever the images it names say about their commits.
    let registry = published();
    registry.untagged(
        CONTROL_PLANE,
        &image_digest(VERSION, "controlPlane"),
        Provenance::Absent,
    );
    registry.image(
        CONSOLE,
        VERSION,
        &digest("rebuilt console"),
        Provenance::Agreed(COMMIT.to_owned()),
    );

    assert_eq!(pinned(&registry).await, Evaluation::Incoherent);
}

#[tokio::test]
async fn the_wrong_version_outranks_everything_after_it() {
    // Not named as primary, not pinned, an image missing: the version is
    // still what is wrong first.
    let registry = published();
    with_attached(&registry, |one| one.version = Some("0.3.0-preview.4".to_owned()));
    registry.delete(CONSOLE, &image_digest(VERSION, "console"));

    assert_eq!(
        pinned_as(&registry, "controlPlane", &pins()).await,
        invalid(InvalidReason::WrongVersion)
    );
}

#[tokio::test]
async fn not_pinned_outranks_a_missing_image() {
    let registry = published();
    registry.delete(CONSOLE, &image_digest(VERSION, "console"));

    assert_eq!(
        pinned_as(&registry, "controlPlane", &pins()).await,
        invalid(InvalidReason::NotPinned)
    );
}

#[tokio::test]
async fn a_registry_that_cannot_be_asked_is_an_error_never_an_answer() {
    // A rate limit that read as undescribed, or an image that could not be
    // read that read as missing, would be the registry's failure reported as
    // a fact about the release.
    for failing in [
        VERSION.to_owned(),
        image_digest(VERSION, "runtime"),
        image_digest(VERSION, "console"),
    ] {
        let registry = published();
        registry.fail_on(&failing);

        let answer = evaluate(
            &registry,
            RUNTIME,
            VERSION,
            Expectation::Pinned {
                primary: "runtime",
                repositories: &pins(),
            },
        )
        .await;

        assert!(
            matches!(answer, Err(RegistryError::Unavailable { .. })),
            "{failing}: {answer:?}"
        );
    }
}

#[test]
fn every_reason_has_its_own_stable_code() {
    let reasons = [
        InvalidReason::Unreadable,
        InvalidReason::UnsupportedVersion {
            found: "v2".to_owned(),
        },
        InvalidReason::WrongVersion,
        InvalidReason::Several,
        InvalidReason::PrimaryNotNamed,
        InvalidReason::OtherRegistry,
        InvalidReason::MissingImage {
            role: "console".to_owned(),
        },
        InvalidReason::NoSingleRevision {
            of: RevisionOf::ComponentDescriptor,
        },
        InvalidReason::NotRegistered {
            repository: CONSOLE.to_owned(),
        },
        InvalidReason::NotPinned,
    ];
    let codes: Vec<&str> = reasons.iter().map(InvalidReason::code).collect();

    assert_eq!(
        codes,
        [
            "unreadable",
            "unsupportedVersion",
            "wrongVersion",
            "several",
            "primaryNotNamed",
            "otherRegistry",
            "missingImage",
            "noSingleRevision",
            "notRegistered",
            "notPinned",
        ]
    );
    for reason in &reasons {
        assert!(!reason.to_string().is_empty());
    }
}
