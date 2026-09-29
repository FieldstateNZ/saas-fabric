//! Every digest this adapter reports is one it computed, and bytes it has
//! verified are content it may reuse — never an answer it may remember.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_platform_management::{Provenance, Registry, RegistryError};
use fabric_registry::OciRegistry;
use support::{sha256, FakeRegistry, Head, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";

fn registry(fake: &FakeRegistry) -> OciRegistry {
    OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5).unwrap()
}

fn refused(failure: &RegistryError) -> &str {
    let RegistryError::Refused { detail } = failure else {
        panic!("expected Refused, got {failure:?}");
    };
    detail
}

#[tokio::test]
async fn a_tag_is_resolved_by_head_and_then_the_bytes_of_the_digest_it_names() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let digest = fake.digest_for(RUNTIME, "0.3.0");

    let resolved = registry(&fake).resolve(RUNTIME, "0.3.0").await.unwrap().unwrap();

    assert_eq!(resolved.digest, digest);
    assert_eq!(fake.count("HEAD", "/manifests/0.3.0"), 1);
    assert_eq!(fake.count("GET", &format!("/manifests/{digest}")), 1);
    assert_eq!(
        fake.count("GET", "/manifests/0.3.0"),
        0,
        "the tag itself is never read"
    );
}

#[tokio::test]
async fn a_registry_that_does_not_answer_head_is_read_by_get_and_hashed() {
    for head in [Head::NotAllowed, Head::WithoutDigest] {
        let fake = FakeRegistry::start().await;
        fake.publish(RUNTIME, "0.3.0", "abc");
        fake.head(head);

        let resolved = registry(&fake).resolve(RUNTIME, "0.3.0").await.unwrap().unwrap();

        assert_eq!(resolved.digest, fake.digest_for(RUNTIME, "0.3.0"));
        assert_eq!(resolved.provenance, Provenance::Agreed("abc".to_owned()));
        assert_eq!(fake.count("GET", "/manifests/0.3.0"), 1);
    }
}

#[tokio::test]
async fn a_digest_header_that_is_not_the_hash_of_the_bytes_is_refused() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    fake.head(Head::NotAllowed);
    fake.digest_header(&sha256(b"something else"));

    let failure = registry(&fake)
        .resolve(RUNTIME, "0.3.0")
        .await
        .expect_err("a lie is refused");

    assert!(refused(&failure).contains("0.3.0"), "{failure:?}");
}

#[tokio::test]
async fn a_digest_in_another_algorithm_is_refused_wherever_it_appears() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let registry = registry(&fake);

    let asked = registry
        .resolve(RUNTIME, &format!("sha512:{}", "a".repeat(128)))
        .await;
    assert!(matches!(asked, Err(RegistryError::Refused { .. })), "{asked:?}");

    fake.digest_header(&format!("sha512:{}", "a".repeat(128)));
    let headed = registry.resolve(RUNTIME, "0.3.0").await;
    assert!(matches!(headed, Err(RegistryError::Refused { .. })), "{headed:?}");
}

#[tokio::test]
async fn a_digest_resolves_to_itself_and_a_mismatch_is_refused() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let digest = fake.digest_for(RUNTIME, "0.3.0");
    let registry = registry(&fake);

    let resolved = registry.resolve(RUNTIME, &digest).await.unwrap().unwrap();
    assert_eq!(resolved.digest, digest);
    assert_eq!(resolved.provenance, Provenance::Agreed("abc".to_owned()));

    let missing = registry
        .resolve(RUNTIME, &sha256(b"never published"))
        .await
        .unwrap();
    assert!(missing.is_none(), "a digest that is not there is an answer");

    fake.corrupt(&digest);
    let failure = registry
        .resolve(RUNTIME, &digest)
        .await
        .expect_err("corrupt bytes are refused");
    assert!(refused(&failure).contains(&digest), "{failure:?}");
}

#[tokio::test]
async fn verified_bytes_are_reused_and_answers_are_asked_again() {
    let fake = FakeRegistry::start().await;
    fake.publish_index(RUNTIME, "0.4.0", &[("amd64", "abc"), ("arm64", "abc")]);
    let registry = registry(&fake);

    let first = registry.resolve(RUNTIME, "0.4.0").await.unwrap().unwrap();
    let reads = fake.count("GET", "/manifests/") + fake.count("GET", "/blobs/");

    let second = registry.resolve(RUNTIME, "0.4.0").await.unwrap().unwrap();

    assert_eq!(first, second);
    assert_eq!(
        fake.count("GET", "/manifests/") + fake.count("GET", "/blobs/"),
        reads,
        "the index, its children and their configs are held by digest"
    );
    assert_eq!(
        fake.count("HEAD", "/manifests/0.4.0"),
        2,
        "the tag is asked every time"
    );

    // The tag goes away: held bytes must not answer for it.
    fake.untag(RUNTIME, "0.4.0");
    assert!(registry.resolve(RUNTIME, "0.4.0").await.unwrap().is_none());
}
