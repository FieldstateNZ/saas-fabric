//! HTTPS, one origin for everything but a blob, and a pull token that never
//! leaves the registry.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_platform_management::{Provenance, Registry, RegistryError};
use fabric_registry::OciRegistry;
use support::{FakeRegistry, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";

fn registry(fake: &FakeRegistry) -> OciRegistry {
    OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5).unwrap()
}

#[test]
fn a_production_client_speaks_https_and_names_the_field_it_refuses_never_its_value() {
    for base_url in [
        "http://127.0.0.1:1",
        "https://user:secret@ghcr.io",
        "https://ghcr.io/?secret",
    ] {
        let Err(message) = OciRegistry::new(base_url, HOST, 5) else {
            panic!("{base_url} is refused");
        };
        assert!(message.contains("base_url"), "{message}");
        assert!(
            !message.contains("secret") && !message.contains("127.0.0.1"),
            "{message}"
        );
    }

    assert!(OciRegistry::new("https://ghcr.io", HOST, 5).is_ok());
    assert!(
        OciRegistry::new("https://ghcr.io", HOST, 0).is_err(),
        "zero is no timeout"
    );
    assert!(OciRegistry::plain_http_to_loopback("http://ghcr.io", HOST, 5).is_err());
}

#[tokio::test]
async fn an_absolute_link_on_the_registrys_own_origin_is_followed() {
    let fake = FakeRegistry::start().await;
    for index in 1..=3 {
        fake.publish(RUNTIME, &format!("0.3.0-preview.{index}"), "abc");
    }
    fake.paginate();
    fake.link_under(&fake.base_url);

    let tags = registry(&fake).tags(RUNTIME).await.unwrap();

    assert_eq!(tags.len(), 3, "got {tags:?}");
}

#[tokio::test]
async fn a_link_to_another_origin_is_refused_rather_than_ending_the_list() {
    let fake = FakeRegistry::start().await;
    for index in 1..=3 {
        fake.publish(RUNTIME, &format!("0.3.0-preview.{index}"), "abc");
    }
    fake.paginate();
    // The same port under another name is another origin.
    fake.link_under(&fake.base_url.replace("127.0.0.1", "localhost"));

    let failure = registry(&fake)
        .tags(RUNTIME)
        .await
        .expect_err("never a short list");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
}

#[tokio::test]
async fn a_manifest_redirected_to_another_origin_is_not_followed() {
    let fake = FakeRegistry::start().await;
    let elsewhere = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let target = format!("{}/v2/fieldstatenz/saas-fabric/manifests/x", elsewhere.base_url);
    fake.answer(
        "GET",
        "/v2/fieldstatenz/saas-fabric/manifests/",
        307,
        &[("Location", target.as_str())],
        "",
    );

    let failure = registry(&fake)
        .resolve(RUNTIME, "0.3.0")
        .await
        .expect_err("refused");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
    assert!(elsewhere.requests().is_empty(), "the pull token went nowhere");
}

#[tokio::test]
async fn a_blob_follows_its_cdn_and_the_pull_token_stays_behind() {
    let fake = FakeRegistry::start_with_cdn().await;
    fake.publish(RUNTIME, "0.3.0", "abc");

    let resolved = registry(&fake).resolve(RUNTIME, "0.3.0").await.unwrap().unwrap();

    // The config's label came back, so the CDN served the blob.
    assert_eq!(resolved.provenance, Provenance::Agreed("abc".to_owned()));

    let asked = fake
        .requests()
        .into_iter()
        .find(|request| request.path.contains("/blobs/"))
        .expect("the registry was asked for the blob");
    assert_eq!(asked.authorization.as_deref(), Some("Bearer token-1"));

    let served = fake.cdn_requests();
    assert_eq!(served.len(), 1, "{served:?}");
    assert_eq!(served[0].authorization, None, "no credential crosses an origin");
}

#[tokio::test]
async fn a_blob_redirected_twice_on_its_cdn_never_carries_the_pull_token() {
    // registry -> CDN -> the same CDN: the last hop is on the host of the hop
    // before it, which is where `reqwest` alone would put the token back.
    let fake = FakeRegistry::start_with_cdn().await;
    fake.cdn_redirects_twice();
    fake.publish(RUNTIME, "0.3.0", "abc");

    let resolved = registry(&fake).resolve(RUNTIME, "0.3.0").await.unwrap().unwrap();

    assert_eq!(resolved.provenance, Provenance::Agreed("abc".to_owned()));
    let served = fake.cdn_requests();
    assert_eq!(served.len(), 2, "{served:?}");
    assert!(
        served.iter().all(|request| request.authorization.is_none()),
        "no credential crosses an origin, on any hop: {served:?}"
    );
}

#[tokio::test]
async fn a_blob_redirected_to_a_location_carrying_a_credential_is_refused_before_it_is_sent() {
    let fake = FakeRegistry::start().await;
    let elsewhere = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    // Another loopback origin, as a CDN's is, and otherwise followable: what
    // makes it refusable is the user and password in its authority, which
    // `reqwest` would send as a `Basic` credential. A signed query rides
    // along, as a CDN's would, so the refusal is checked against both.
    let target = format!(
        "{}/blobs/sha256:0?X-Amz-Signature=signedquery",
        elsewhere
            .base_url
            .replacen("http://", "http://cdnuser:s3cret@", 1)
    );
    fake.answer(
        "GET",
        "/v2/fieldstatenz/saas-fabric/blobs/",
        307,
        &[("Location", target.as_str())],
        "",
    );

    let failure = registry(&fake)
        .resolve(RUNTIME, "0.3.0")
        .await
        .expect_err("a credential in a redirect is refused");

    let RegistryError::Refused { detail } = failure else {
        panic!("expected Refused, got {failure:?}");
    };
    // On failure, name the part of the target the detail reflects, never the
    // value itself nor the detail that carries it.
    let port = elsewhere.base_url.rsplit(':').next().unwrap();
    for (part, secret) in [
        ("user", "cdnuser"),
        ("password", "s3cret"),
        ("query key", "X-Amz"),
        ("query value", "signedquery"),
        ("path", "sha256:0"),
        ("port", port),
    ] {
        assert!(
            !detail.contains(secret),
            "the refusal reflects the redirect target's {part}"
        );
    }
    assert_eq!(
        elsewhere.requests().len(),
        0,
        "the target was never contacted, yet it saw requests"
    );
    assert_eq!(
        fake.count("GET", "/blobs/"),
        1,
        "no hop was sent: {:?}",
        fake.paths()
    );
}

#[tokio::test]
async fn a_credential_in_a_redirect_to_the_registrys_own_origin_is_refused_too() {
    // Even on the origin allowed to receive the pull token, userinfo in
    // the redirect target is refused before another request is built.
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let target = format!(
        "{}/v2/fieldstatenz/saas-fabric/blobs/sha256:0",
        fake.base_url.replacen("http://", "http://cdnuser:s3cret@", 1)
    );
    fake.answer(
        "GET",
        "/v2/fieldstatenz/saas-fabric/blobs/",
        307,
        &[("Location", target.as_str())],
        "",
    );

    let failure = registry(&fake)
        .resolve(RUNTIME, "0.3.0")
        .await
        .expect_err("a credential in a redirect is refused");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
    // Exactly the one blob request that was redirected: the redirect target,
    // though on this same origin, was never contacted.
    assert_eq!(
        fake.count("GET", "/blobs/"),
        1,
        "no hop was sent: {:?}",
        fake.paths()
    );
    // The first manifest read goes anonymously and is challenged, so only
    // the blob requests are held to carrying the pull token and nothing else.
    assert!(
        fake.requests()
            .iter()
            .filter(|request| request.path.contains("/blobs/"))
            .all(|request| request.authorization.as_deref() == Some("Bearer token-1")),
        "no blob request carried anything but the pull token: {:?}",
        fake.requests()
    );
}

#[tokio::test]
async fn a_blob_redirected_in_a_circle_is_refused_after_ten_hops_each_carrying_the_token() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    // Every blob request, including the one this redirects to, is answered
    // by this same redirect: a chain with no end, on the registry's own
    // origin, where the bound is the only thing that stops it.
    let target = "/v2/fieldstatenz/saas-fabric/blobs/sha256:0?X-Amz-Signature=loopsig";
    fake.answer(
        "GET",
        "/v2/fieldstatenz/saas-fabric/blobs/",
        307,
        &[("Location", target)],
        "",
    );

    let failure = registry(&fake)
        .resolve(RUNTIME, "0.3.0")
        .await
        .expect_err("a circle is refused, never followed until the timeout");

    // Refused, not Unavailable: asking again gets the same circle.
    let RegistryError::Refused { detail } = failure else {
        panic!("expected Refused, got {failure:?}");
    };
    assert!(detail.contains("more than 10 redirects"), "{detail}");
    assert!(
        !detail.contains("loopsig") && !detail.contains("sha256:0"),
        "{detail:?} names the target"
    );
    let hops: Vec<_> = fake
        .requests()
        .into_iter()
        .filter(|request| request.path.contains("/blobs/"))
        .collect();
    assert_eq!(
        hops.len(),
        11,
        "the first request and ten redirects, never an eleventh: {hops:?}"
    );
    assert!(
        hops.iter()
            .all(|request| request.authorization.as_deref() == Some("Bearer token-1")),
        "every hop on the registry's own origin carries the pull token: {hops:?}"
    );
}

#[tokio::test]
async fn a_tag_listing_whose_later_page_is_not_there_is_an_error_not_a_short_list() {
    let fake = FakeRegistry::start().await;
    for index in 1..=3 {
        fake.publish(RUNTIME, &format!("0.3.0-preview.{index}"), "abc");
    }
    fake.paginate();
    fake.answer(
        "GET",
        "/v2/fieldstatenz/saas-fabric/tags/list?last=1",
        404,
        &[],
        "{}",
    );

    let failure = registry(&fake).tags(RUNTIME).await;

    assert!(failure.is_err(), "never the first page alone: {failure:?}");
}

#[tokio::test]
async fn a_token_named_access_token_is_presented_like_any_other() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    fake.token_key("access_token");

    let resolved = registry(&fake).resolve(RUNTIME, "0.3.0").await.unwrap();

    assert!(resolved.is_some());
    // The first read goes with nothing, and is challenged; every one after
    // it presents the token the realm named `access_token`.
    let presented: Vec<_> = fake
        .requests()
        .into_iter()
        .filter(|request| request.path.starts_with("/v2/"))
        .skip(1)
        .collect();
    assert!(!presented.is_empty());
    assert!(presented
        .iter()
        .all(|request| request.authorization.as_deref() == Some("Bearer token-1")));
}
