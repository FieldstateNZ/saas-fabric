//! Every body is read within a bound before it is parsed: an unbounded read
//! of a remote document is an unbounded allocation decided by somebody else.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_platform_management::{Registry, RegistryError};
use fabric_registry::OciRegistry;
use serde_json::json;
use support::{FakeRegistry, Listed, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const INDEX: &str = "application/vnd.oci.image.index.v1+json";

fn registry(fake: &FakeRegistry) -> OciRegistry {
    OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5).unwrap()
}

/// Refused, and for its size rather than anything else.
fn assert_refused<T: std::fmt::Debug>(result: &Result<T, RegistryError>) {
    let Err(RegistryError::Refused { detail }) = result else {
        panic!("expected Refused, got {result:?}");
    };
    assert!(detail.contains("more than"), "{detail}");
}

/// `n` bytes of padding inside a JSON string.
fn padding(n: usize) -> String {
    "a".repeat(n)
}

#[tokio::test]
async fn a_token_response_past_sixteen_kibibytes_is_refused() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    fake.answer(
        "GET",
        "/token",
        200,
        &[],
        json!({ "token": "t", "pad": padding(16 * 1024) }).to_string(),
    );

    assert_refused(&registry(&fake).resolve(RUNTIME, "0.3.0").await);
}

#[tokio::test]
async fn a_tag_page_past_one_mebibyte_is_refused() {
    let fake = FakeRegistry::start().await;
    fake.answer(
        "GET",
        "/v2/fieldstatenz/saas-fabric/tags/list",
        200,
        &[],
        json!({ "tags": [padding(1024 * 1024)] }).to_string(),
    );

    assert_refused(&registry(&fake).tags(RUNTIME).await);
}

#[tokio::test]
async fn a_manifest_past_four_mebibytes_is_refused() {
    let fake = FakeRegistry::start().await;
    let huge = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": { "digest": "sha256:0", "size": 2 },
        "annotations": { "pad": padding(4 * 1024 * 1024) }
    });
    let digest = fake.put_manifest(RUNTIME, &huge.to_string());
    fake.tag(RUNTIME, "0.3.0", &digest);

    assert_refused(&registry(&fake).resolve(RUNTIME, "0.3.0").await);
}

#[tokio::test]
async fn a_config_blob_past_one_mebibyte_is_refused() {
    let fake = FakeRegistry::start().await;
    let config = json!({ "config": { "Labels": { "pad": padding(1024 * 1024) } } }).to_string();
    let (config_digest, size) = fake.put_blob(RUNTIME, &config);
    let manifest = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": { "digest": config_digest, "size": size },
    });
    let digest = fake.put_manifest(RUNTIME, &manifest.to_string());
    fake.tag(RUNTIME, "0.3.0", &digest);

    assert_refused(&registry(&fake).resolve(RUNTIME, "0.3.0").await);
}

#[tokio::test]
async fn a_referrers_page_past_one_mebibyte_is_refused() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let subject = fake.digest_for(RUNTIME, "0.3.0");
    fake.answer(
        "GET",
        "/v2/fieldstatenz/saas-fabric/referrers/",
        200,
        &[("Content-Type", INDEX)],
        json!({ "schemaVersion": 2, "mediaType": INDEX, "manifests": [], "pad": padding(1024 * 1024) })
            .to_string(),
    );

    assert_refused(&registry(&fake).component_descriptor(RUNTIME, &subject).await);
}

#[tokio::test]
async fn a_component_descriptor_past_its_bound_is_refused_whatever_its_manifest_declares() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let subject = fake.digest_for(RUNTIME, "0.3.0");

    // The manifest declares a small layer; the registry serves a large one.
    let mut manifest = fake.descriptor_manifest(RUNTIME, &subject, &padding(20 * 1024));
    manifest["layers"][0]["size"] = json!(100);
    fake.attach_raw(RUNTIME, &subject, &manifest, Listed::TagSchema);

    assert_refused(&registry(&fake).component_descriptor(RUNTIME, &subject).await);
}

#[tokio::test]
async fn a_token_response_with_no_declared_length_is_bounded_as_it_arrives() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    fake.unlengthed();
    fake.answer(
        "GET",
        "/token",
        200,
        &[],
        json!({ "token": "t", "pad": padding(16 * 1024) }).to_string(),
    );

    assert_refused(&registry(&fake).resolve(RUNTIME, "0.3.0").await);
}

#[tokio::test]
async fn a_component_descriptor_with_no_declared_length_is_bounded_as_it_arrives() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    fake.unlengthed();
    let subject = fake.digest_for(RUNTIME, "0.3.0");
    let mut manifest = fake.descriptor_manifest(RUNTIME, &subject, &padding(20 * 1024));
    manifest["layers"][0]["size"] = json!(100);
    fake.attach_raw(RUNTIME, &subject, &manifest, Listed::TagSchema);

    assert_refused(&registry(&fake).component_descriptor(RUNTIME, &subject).await);
}

#[tokio::test]
async fn an_unlengthed_answer_within_its_bound_is_read_whole() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    fake.unlengthed();

    let resolved = registry(&fake).resolve(RUNTIME, "0.3.0").await.unwrap();

    assert!(resolved.is_some());
}
