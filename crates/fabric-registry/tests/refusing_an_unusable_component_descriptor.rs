//! What is attached and cannot be used is said so, for a closed reason.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_platform_management::{Attached, Registry, RegistryError, Unusable};
use fabric_registry::OciRegistry;
use serde_json::json;
use support::{sha256, FakeRegistry, Listed, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const DOCUMENT: &str = r#"{"apiVersion":"fabric.fieldstate.nz/v1","kind":"Component","spec":{}}"#;

fn registry(fake: &FakeRegistry) -> OciRegistry {
    OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5).unwrap()
}

async fn published() -> (FakeRegistry, String) {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "5320432");
    let subject = fake.digest_for(RUNTIME, "0.3.0");
    (fake, subject)
}

async fn unusable(fake: &FakeRegistry, subject: &str) -> Unusable {
    match registry(fake)
        .component_descriptor(RUNTIME, subject)
        .await
        .unwrap()
    {
        Attached::Unusable { reason } => reason,
        other => panic!("expected Unusable, got {other:?}"),
    }
}

#[tokio::test]
async fn a_referrers_tag_that_is_not_an_index_is_not_an_index() {
    let (fake, subject) = published().await;
    let image = fake.image(RUNTIME, Some("5320432"), None, "amd64");
    let body = json!({ "schemaVersion": 2, "mediaType": "application/vnd.oci.image.manifest.v1+json",
                       "config": { "digest": image } });
    fake.tag_schema_holds(RUNTIME, &subject, &body.to_string());

    assert_eq!(unusable(&fake, &subject).await, Unusable::NotAnIndex);
}

#[tokio::test]
async fn a_referrers_tag_holding_an_image_is_not_an_index_on_a_registry_that_negotiates_strictly() {
    // CNCF distribution answers `404` for a stored type the request did not
    // accept, so asking for an index alone would read this as nothing.
    let (fake, subject) = published().await;
    fake.strict_accept();
    let image = fake.image(RUNTIME, Some("5320432"), None, "amd64");
    let body = json!({ "schemaVersion": 2, "mediaType": "application/vnd.oci.image.manifest.v1+json",
                       "config": { "digest": image } });
    fake.tag_schema_holds(RUNTIME, &subject, &body.to_string());

    assert_eq!(unusable(&fake, &subject).await, Unusable::NotAnIndex);
}

#[tokio::test]
async fn a_component_descriptor_naming_another_subject_is_named_as_such() {
    let (fake, subject) = published().await;
    let other = sha256(b"another image");
    let manifest = fake.descriptor_manifest(RUNTIME, &other, DOCUMENT);
    fake.attach_raw(RUNTIME, &subject, &manifest, Listed::TagSchema);

    assert_eq!(unusable(&fake, &subject).await, Unusable::OtherSubject);
}

#[tokio::test]
async fn a_component_descriptor_of_the_wrong_shape_is_malformed() {
    let (fake, subject) = published().await;
    let mut manifest = fake.descriptor_manifest(RUNTIME, &subject, DOCUMENT);
    let layer = manifest["layers"][0].clone();
    manifest["layers"] = json!([layer.clone(), layer]);
    fake.attach_raw(RUNTIME, &subject, &manifest, Listed::TagSchema);

    assert_eq!(unusable(&fake, &subject).await, Unusable::Malformed);
}

#[tokio::test]
async fn a_component_descriptor_whose_layer_is_not_served_is_malformed() {
    let (fake, subject) = published().await;
    let mut manifest = fake.descriptor_manifest(RUNTIME, &subject, DOCUMENT);
    manifest["layers"][0]["digest"] = sha256(b"never pushed").into();
    fake.attach_raw(RUNTIME, &subject, &manifest, Listed::TagSchema);

    assert_eq!(unusable(&fake, &subject).await, Unusable::Malformed);
}

#[tokio::test]
async fn a_layer_that_is_not_its_declared_size_is_malformed_not_an_error() {
    // The publisher's fault in one version: an error would stop every
    // discovery pass until someone deleted the artifact.
    let (fake, subject) = published().await;
    let mut manifest = fake.descriptor_manifest(RUNTIME, &subject, DOCUMENT);
    manifest["layers"][0]["size"] = json!(DOCUMENT.len() + 1);
    fake.attach_raw(RUNTIME, &subject, &manifest, Listed::TagSchema);

    assert_eq!(unusable(&fake, &subject).await, Unusable::Malformed);
}

#[tokio::test]
async fn a_layer_that_does_not_hash_to_its_digest_is_still_refused() {
    let (fake, subject) = published().await;
    let manifest = fake.descriptor_manifest(RUNTIME, &subject, DOCUMENT);
    let layer = manifest["layers"][0]["digest"].as_str().unwrap().to_owned();
    fake.attach_raw(RUNTIME, &subject, &manifest, Listed::TagSchema);
    fake.corrupt(&layer);

    let failure = registry(&fake).component_descriptor(RUNTIME, &subject).await;

    assert!(
        matches!(failure, Err(RegistryError::Refused { .. })),
        "{failure:?}"
    );
}

#[tokio::test]
async fn more_candidates_than_are_fetched_are_several_without_fetching_any() {
    let (fake, subject) = published().await;
    for index in 0..17 {
        let digest = sha256(format!("candidate {index}").as_bytes());
        fake.list(
            RUNTIME,
            &subject,
            &digest,
            fabric_component::ARTIFACT_TYPE,
            10,
            Listed::TagSchema,
        );
    }

    let found = registry(&fake)
        .component_descriptor(RUNTIME, &subject)
        .await
        .unwrap();

    let Attached::Several { digests } = found else {
        panic!("expected Several, got {found:?}");
    };
    assert!(
        digests.is_empty(),
        "none was computed, so none is carried: {digests:?}"
    );
    assert_eq!(fake.count("GET", "/manifests/sha256:"), 0, "none was fetched");
}

#[tokio::test]
async fn a_subject_that_is_not_a_sha256_digest_is_refused() {
    let (fake, _) = published().await;

    let failure = registry(&fake).component_descriptor(RUNTIME, "0.3.0").await;

    assert!(
        matches!(failure, Err(RegistryError::Refused { .. })),
        "{failure:?}"
    );
}
