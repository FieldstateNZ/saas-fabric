//! A registry an operator registered is read at public addresses only: every
//! name through a resolver that hands back nothing else, and never an IP
//! literal, wherever one is named.
//!
//! Which addresses count as public is tested beside the resolver, with an
//! injected lookup; these prove the resolver and the literal check are on
//! the wire, using the test switch that counts loopback as public to reach a
//! socket at all.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_platform_management::{Registry, RegistryError};
use fabric_registry::{OciRegistry, RealmRule, RegistrySettings};
use support::{localhost, Challenge, FakeRegistry};

const REPOSITORY: &str = "registry.example/team/app";

fn settings(fake: &FakeRegistry) -> RegistrySettings {
    RegistrySettings::distribution("https://registry.example", RealmRule::FollowChallenge)
        .unwrap()
        .serve_from(&localhost(&fake.base_url))
        .unwrap()
}

/// A registry whose realm is named `localhost`, as a public one would be
/// named by a host.
fn named(fake: &FakeRegistry) {
    fake.challenge(Challenge::Bearer {
        realm: localhost(&format!("{}/token", fake.base_url)),
        service: "registry.example".to_owned(),
    });
}

fn refusal<T: std::fmt::Debug>(result: Result<T, RegistryError>) -> String {
    match result {
        Err(RegistryError::Refused { detail }) => detail,
        other => panic!("expected Refused, got {other:?}"),
    }
}

#[tokio::test]
async fn a_name_resolving_to_loopback_is_never_dialled() {
    let fake = FakeRegistry::start().await;
    fake.publish("team/app", "1.0.0", "abc");
    let registry = OciRegistry::with_settings(settings(&fake), 5).unwrap();

    let detail = refusal(registry.tags(REPOSITORY).await);

    assert!(detail.contains("public addresses"), "{detail}");
    assert!(fake.requests().is_empty(), "no connection reached the socket");
}

#[tokio::test]
async fn the_same_registry_is_read_once_loopback_counts_as_public() {
    let fake = FakeRegistry::start().await;
    named(&fake);
    fake.publish("team/app", "1.0.0", "abc");
    let registry = OciRegistry::with_settings(settings(&fake).treat_loopback_as_public(), 5).unwrap();

    assert_eq!(registry.tags(REPOSITORY).await.unwrap(), vec!["1.0.0".to_owned()]);
}

#[tokio::test]
async fn an_ip_literal_realm_blob_redirect_or_endpoint_is_refused_before_any_request() {
    // A realm named by address.
    let fake = FakeRegistry::start().await;
    fake.publish("team/app", "1.0.0", "abc");
    let registry = OciRegistry::with_settings(settings(&fake).treat_loopback_as_public(), 5).unwrap();
    let realm = refusal(registry.tags(REPOSITORY).await);
    assert_eq!(fake.mints(), 0, "the realm was never asked");

    // A blob redirected to a CDN named by address.
    let cdn = FakeRegistry::start_with_cdn().await;
    named(&cdn);
    cdn.publish("team/app", "1.0.0", "abc");
    let registry = OciRegistry::with_settings(settings(&cdn).treat_loopback_as_public(), 5).unwrap();
    let redirect = refusal(registry.resolve(REPOSITORY, "1.0.0").await);
    assert!(cdn.cdn_requests().is_empty(), "the CDN was never asked");

    // A resolved name, for comparison: every refusal reads the same.
    let unswitched = OciRegistry::with_settings(settings(&fake), 5).unwrap();
    let resolved = refusal(unswitched.tags(REPOSITORY).await);
    assert_eq!(realm, redirect);
    assert_eq!(realm, resolved);

    // An endpoint named by address is refused when the client is built.
    let literal = RegistrySettings::distribution("https://registry.example", RealmRule::FollowChallenge)
        .unwrap()
        .serve_from(&fake.base_url)
        .unwrap()
        .treat_loopback_as_public();
    assert!(OciRegistry::with_settings(literal, 5).is_err());
}

#[tokio::test]
async fn a_manifest_redirected_to_an_ip_literal_is_refused_the_same_way() {
    let fake = FakeRegistry::start().await;
    named(&fake);
    fake.publish("team/app", "1.0.0", "abc");
    let target = format!("{}/v2/team/app/manifests/x", fake.base_url);
    fake.answer(
        "GET",
        "/v2/team/app/manifests/",
        307,
        &[("Location", target.as_str())],
        "",
    );
    let registry = OciRegistry::with_settings(settings(&fake).treat_loopback_as_public(), 5).unwrap();

    let detail = refusal(registry.resolve(REPOSITORY, "1.0.0").await);

    assert!(detail.contains("public addresses"), "{detail}");
}
