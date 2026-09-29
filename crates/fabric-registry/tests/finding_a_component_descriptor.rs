//! Finding the component descriptor attached to an image, through the
//! referrers API and through the referrers tag schema.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use fabric_platform_management::{Attached, Registry, RegistryError};
use fabric_registry::OciRegistry;
use support::{sha256, FakeRegistry, Listed, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const DOCUMENT: &str = r#"{"apiVersion":"fabric.fieldstate.nz/v1","kind":"Component","spec":{}}"#;
const OTHER_DOCUMENT: &str = r#"{"apiVersion":"fabric.fieldstate.nz/v1","kind":"Component","spec":{"x":1}}"#;
const SIGNATURE: &str = "application/vnd.cncf.notary.signature";

fn registry(fake: &FakeRegistry) -> OciRegistry {
    OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5).unwrap()
}

/// A registry with the image `0.3.0` published, and its digest.
async fn published(serves_api: bool) -> (FakeRegistry, String) {
    let fake = FakeRegistry::start().await;
    if serves_api {
        fake.serve_referrers_api();
    }
    fake.publish(RUNTIME, "0.3.0", "5320432");
    let subject = fake.digest_for(RUNTIME, "0.3.0");
    (fake, subject)
}

#[tokio::test]
async fn through_the_api_the_one_attached_is_returned_byte_for_byte() {
    let (fake, subject) = published(true).await;
    let digest = fake.attach(RUNTIME, "0.3.0", DOCUMENT, Listed::Api);

    let Attached::One(found) = registry(&fake)
        .component_descriptor(RUNTIME, &subject)
        .await
        .unwrap()
    else {
        panic!("one attached");
    };

    assert_eq!(found.digest, digest);
    assert_eq!(found.artifact_type, fabric_component::ARTIFACT_TYPE);
    assert_eq!(found.revision.as_deref(), Some("5320432"));
    assert_eq!(found.version.as_deref(), Some("1.4.0"));
    assert_eq!(found.document, DOCUMENT.as_bytes());
}

#[tokio::test]
async fn where_the_api_is_not_served_the_tag_schema_is_read() {
    // GHCR answers the referrers API `404 MANIFEST_UNKNOWN`.
    let (fake, subject) = published(false).await;
    let digest = fake.attach(RUNTIME, "0.3.0", DOCUMENT, Listed::TagSchema);

    let Attached::One(found) = registry(&fake)
        .component_descriptor(RUNTIME, &subject)
        .await
        .unwrap()
    else {
        panic!("one attached");
    };

    assert_eq!(found.digest, digest);
    assert_eq!(found.document, DOCUMENT.as_bytes());
}

#[tokio::test]
async fn an_api_answer_that_is_not_an_index_means_the_api_is_not_served() {
    let (fake, subject) = published(false).await;
    fake.attach(RUNTIME, "0.3.0", DOCUMENT, Listed::TagSchema);
    fake.answer("GET", "/v2/fieldstatenz/saas-fabric/referrers/", 200, &[], "{}");

    let found = registry(&fake)
        .component_descriptor(RUNTIME, &subject)
        .await
        .unwrap();

    assert!(matches!(found, Attached::One(_)), "{found:?}");
}

#[tokio::test]
async fn both_lists_are_read_and_merged_by_digest() {
    let (fake, subject) = published(true).await;
    fake.attach(RUNTIME, "0.3.0", DOCUMENT, Listed::Both);
    let registry = registry(&fake);

    let once = registry.component_descriptor(RUNTIME, &subject).await.unwrap();
    assert!(
        matches!(once, Attached::One(_)),
        "one digest listed twice is one: {once:?}"
    );

    // One attached before the registry served the API, one after.
    let older = fake.attach(RUNTIME, "0.3.0", OTHER_DOCUMENT, Listed::TagSchema);
    let found = registry.component_descriptor(RUNTIME, &subject).await.unwrap();

    let Attached::Several { digests } = found else {
        panic!("two are several: {found:?}");
    };
    assert!(digests.contains(&older) && digests.len() == 2, "{digests:?}");
    assert!(
        digests.windows(2).all(|pair| pair[0] < pair[1]),
        "sorted: {digests:?}"
    );
}

#[tokio::test]
async fn nothing_attached_is_nothing() {
    let (fake, subject) = published(true).await;

    let found = registry(&fake)
        .component_descriptor(RUNTIME, &subject)
        .await
        .unwrap();

    assert_eq!(found, Attached::Nothing);
}

#[tokio::test]
async fn other_referrers_are_ignored_and_never_fetched() {
    let (fake, subject) = published(true).await;
    let mut signature = fake.descriptor_manifest(RUNTIME, &subject, "signature");
    signature["artifactType"] = SIGNATURE.into();
    let signature = fake.attach_raw(RUNTIME, &subject, &signature, Listed::Api);
    fake.attach(RUNTIME, "0.3.0", DOCUMENT, Listed::Api);

    let found = registry(&fake)
        .component_descriptor(RUNTIME, &subject)
        .await
        .unwrap();

    assert!(matches!(found, Attached::One(_)), "{found:?}");
    assert_eq!(fake.count("GET", &signature), 0);
}

#[tokio::test]
async fn a_listed_referrer_whose_manifest_is_gone_is_not_attached() {
    let (fake, subject) = published(false).await;
    let gone = sha256(b"deleted since it was listed");
    fake.list(
        RUNTIME,
        &subject,
        &gone,
        fabric_component::ARTIFACT_TYPE,
        10,
        Listed::TagSchema,
    );
    fake.attach(RUNTIME, "0.3.0", DOCUMENT, Listed::TagSchema);

    let found = registry(&fake)
        .component_descriptor(RUNTIME, &subject)
        .await
        .unwrap();

    assert!(matches!(found, Attached::One(_)), "{found:?}");
}

#[tokio::test]
async fn a_repository_that_does_not_exist_is_an_error_naming_it() {
    let (fake, subject) = published(true).await;
    let body = r#"{"errors":[{"code":"NAME_UNKNOWN","message":"repository name not known"}]}"#;
    fake.answer("GET", "/v2/fieldstatenz/saas-fabric/referrers/", 404, &[], body);

    let failure = registry(&fake)
        .component_descriptor(RUNTIME, &subject)
        .await
        .expect_err("not nothing attached");

    let RegistryError::Refused { detail } = failure else {
        panic!("expected Refused, got {failure:?}");
    };
    assert!(detail.contains("fieldstatenz/saas-fabric"), "{detail}");
}

#[tokio::test]
async fn a_registry_that_cannot_be_asked_is_never_nothing_attached() {
    let (fake, subject) = published(true).await;
    fake.answer("GET", "/v2/fieldstatenz/saas-fabric/referrers/", 429, &[], "{}");

    let failure = registry(&fake).component_descriptor(RUNTIME, &subject).await;

    assert!(
        matches!(failure, Err(RegistryError::Unavailable { .. })),
        "{failure:?}"
    );
}
