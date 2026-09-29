//! A token is asked of the realm the registry's kind allows, and of no other,
//! whatever a challenge names.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_platform_management::{Registry, RegistryError};
use fabric_registry::{AddressPolicy, Credential, OciRegistry, RealmRule, RegistrySecret, RegistrySettings};
use support::{localhost, Challenge, FakeRegistry, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const CONSOLE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane-ui";

/// The refusal's text, which must be `Refused`.
fn refused<T: std::fmt::Debug>(result: Result<T, RegistryError>) -> String {
    match result {
        Err(RegistryError::Refused { detail }) => detail,
        other => panic!("expected Refused, got {other:?}"),
    }
}

/// The origin of a fake's address: `http://127.0.0.1:<port>`.
fn origin_of(url: &str) -> String {
    let url = reqwest::Url::parse(url).unwrap();
    url.origin().ascii_serialization()
}

#[tokio::test]
async fn the_deployments_registry_follows_its_first_challenge_and_refuses_a_changed_one() {
    let fake = FakeRegistry::start_with_realm().await;
    let elsewhere = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    fake.publish(CONSOLE, "0.3.0", "abc");
    let registry = OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5).unwrap();

    registry.resolve(RUNTIME, "0.3.0").await.unwrap().unwrap();
    assert!(
        !fake.realm_requests().is_empty(),
        "the realm on another port was asked"
    );
    let first = fake.realm();

    fake.challenge(Challenge::Bearer {
        realm: format!("{}/token", elsewhere.base_url),
        service: HOST.to_owned(),
    });
    let detail = refused(registry.resolve(CONSOLE, "0.3.0").await);

    assert!(detail.contains(&origin_of(&first)), "{detail}");
    assert!(detail.contains(&origin_of(&elsewhere.base_url)), "{detail}");
    assert!(!detail.contains("/token"), "an origin, never a path: {detail}");
    assert!(
        elsewhere.requests().is_empty(),
        "nothing is sent to a changed realm"
    );
}

#[tokio::test]
async fn a_distribution_registry_asks_the_realm_it_recorded_and_no_other() {
    let fake = FakeRegistry::start_with_realm().await;
    let elsewhere = FakeRegistry::start().await;
    fake.publish("team/app", "1.0.0", "abc");
    let recorded = localhost(&origin_of(&fake.realm()));
    fake.challenge(Challenge::Bearer {
        realm: format!("{recorded}/token"),
        service: "registry.example".to_owned(),
    });
    let settings = RegistrySettings::distribution(
        "https://registry.example",
        RealmRule::Recorded {
            origin: Some(recorded.clone()),
        },
    )
    .unwrap()
    .serve_from(&localhost(&fake.base_url))
    .unwrap()
    .treat_loopback_as_public();
    let registry = OciRegistry::with_settings(settings, 5).unwrap();

    let tags = registry.tags("registry.example/team/app").await.unwrap();
    assert_eq!(tags, vec!["1.0.0".to_owned()]);

    let changed = localhost(&format!("{}/token", elsewhere.base_url));
    fake.challenge(Challenge::Bearer {
        realm: changed.clone(),
        service: "registry.example".to_owned(),
    });
    let detail = refused(registry.tags("registry.example/team/other").await);

    assert!(detail.contains(&recorded), "{detail}");
    assert!(detail.contains(&origin_of(&changed)), "{detail}");
    assert!(
        elsewhere.requests().is_empty(),
        "nothing is sent to a changed realm"
    );
}

#[tokio::test]
async fn a_hosted_kind_refuses_a_challenge_naming_any_realm_but_its_own() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    // Named by a host, as a real realm is: an address would be refused
    // before the rule was consulted.
    let named = localhost(&format!("{}/token", fake.base_url));
    fake.challenge(Challenge::Bearer {
        realm: named.clone(),
        service: HOST.to_owned(),
    });
    let settings = RegistrySettings::ghcr()
        .serve_from(&localhost(&fake.base_url))
        .unwrap()
        .treat_loopback_as_public();
    let registry = OciRegistry::with_settings(settings, 5).unwrap();

    let detail = refused(registry.resolve(RUNTIME, "0.3.0").await);

    assert!(detail.contains("https://ghcr.io"), "{detail}");
    assert!(detail.contains(&origin_of(&named)), "{detail}");
    assert_eq!(
        fake.mints(),
        0,
        "no token was asked of the realm the challenge named"
    );
}

#[tokio::test]
async fn a_registry_that_never_challenges_is_read_with_nothing_presented() {
    let fake = FakeRegistry::start().await;
    fake.challenge(Challenge::None);
    fake.publish(RUNTIME, "0.3.0", "abc");
    let registry = OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5).unwrap();

    registry.resolve(RUNTIME, "0.3.0").await.unwrap().unwrap();

    assert_eq!(fake.mints(), 0);
    assert!(fake
        .requests()
        .iter()
        .all(|request| request.authorization.is_none()));
}

#[tokio::test]
async fn a_deployment_hosted_kind_keeps_its_fixed_realm() {
    // An operator's `ghcr` credential for the deployment's own host: served
    // where the deployment reads it, and still sent only to GHCR's realm.
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let credential = Credential::new("brett", RegistrySecret::new("a-token"), [RUNTIME]).unwrap();
    let settings = RegistrySettings::ghcr()
        .with_credential(credential)
        .at_deployment("https://ghcr.io")
        .unwrap()
        .serve_from(&fake.base_url)
        .unwrap();
    assert_eq!(settings.address(), AddressPolicy::Any);
    let registry = OciRegistry::with_settings(settings, 5).unwrap();

    let detail = refused(registry.resolve(RUNTIME, "0.3.0").await);

    assert!(detail.contains("https://ghcr.io"), "{detail}");
    assert!(detail.contains(&origin_of(&fake.realm())), "{detail}");
    assert_eq!(fake.mints(), 0, "the credential went nowhere");
}

#[tokio::test]
async fn a_followed_realm_is_recorded_only_once_it_could_be_asked() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let good = fake.realm();
    fake.challenge(Challenge::Bearer {
        realm: "http://auth.example/token".to_owned(),
        service: HOST.to_owned(),
    });
    let registry = OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5).unwrap();

    let detail = refused(registry.resolve(RUNTIME, "0.3.0").await);
    assert!(detail.contains("not an HTTPS address"), "{detail}");

    fake.challenge(Challenge::Bearer {
        realm: good,
        service: HOST.to_owned(),
    });
    registry
        .resolve(RUNTIME, "0.3.0")
        .await
        .unwrap()
        .expect("the realm refused first was never recorded");
}
