//! A credential is presented for the repositories it was registered for,
//! to the realm its kind allows, and — refused — not again.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use fabric_platform_management::{Provenance, Registry, RegistryError};
use fabric_registry::{Credential, OciRegistry, RealmRule, RegistrySecret, RegistrySettings};
use support::{basic, localhost, Challenge, FakeRegistry, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const CONSOLE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane-ui";
const USERNAME: &str = "brett";
const SECRET: &str = "ghp_not-a-real-token-but-secret";

fn credential(repositories: &[&str]) -> Credential {
    Credential::new(
        USERNAME,
        RegistrySecret::new(SECRET),
        repositories.iter().copied(),
    )
    .unwrap()
}

/// The deployment's registry, holding a credential for `repositories`.
fn deployment(fake: &FakeRegistry, repositories: &[&str]) -> OciRegistry {
    let settings = RegistrySettings::deployment(&fake.base_url, HOST)
        .unwrap()
        .serve_from(&fake.base_url)
        .unwrap()
        .with_credential(credential(repositories));
    OciRegistry::with_settings(settings, 5).unwrap()
}

/// A distribution registry recorded with no realm, holding a credential for
/// `repositories`.
fn distribution(fake: &FakeRegistry, repositories: &[&str]) -> OciRegistry {
    let settings =
        RegistrySettings::distribution("https://registry.example", RealmRule::Recorded { origin: None })
            .unwrap()
            .serve_from(&localhost(&fake.base_url))
            .unwrap()
            .treat_loopback_as_public()
            .with_credential(credential(repositories));
    OciRegistry::with_settings(settings, 5).unwrap()
}

/// Whether anything printed from `failure` carries the secret.
fn assert_no_secret(failure: &RegistryError) {
    let encoded = basic(USERNAME, SECRET);
    for printed in [format!("{failure}"), format!("{failure:?}")] {
        assert!(!printed.contains(SECRET), "{printed}");
        assert!(!printed.contains(&encoded["Basic ".len()..]), "{printed}");
    }
}

#[tokio::test]
async fn the_credential_goes_to_the_realm_only_for_the_repositories_it_was_registered_for() {
    let fake = FakeRegistry::start_with_realm().await;
    fake.expect_credential(USERNAME, SECRET);
    fake.publish(RUNTIME, "0.3.0", "abc");
    fake.publish(CONSOLE, "0.3.0", "abc");
    let registry = deployment(&fake, &[RUNTIME]);

    registry.resolve(RUNTIME, "0.3.0").await.unwrap().unwrap();
    registry.resolve(CONSOLE, "0.3.0").await.unwrap().unwrap();

    let asked = fake.realm_requests();
    let for_path = |path: &str| {
        let scope = format!("scope=repository%3Afieldstatenz%2F{path}%3Apull");
        asked
            .iter()
            .filter(|request| request.path.contains(&scope))
            .collect::<Vec<_>>()
    };
    let runtime = for_path("saas-fabric");
    let console = for_path("saas-fabric-control-plane-ui");
    assert_eq!(runtime.len(), 1, "{asked:?}");
    assert_eq!(console.len(), 1, "{asked:?}");
    assert_eq!(runtime[0].authorization, Some(basic(USERNAME, SECRET)));
    assert_eq!(
        console[0].authorization, None,
        "an anonymous repository is read anonymously"
    );

    // Tokens are held apart: the console never borrows the credentialed one.
    let bearer = |fragment: &str| {
        fake.requests()
            .into_iter()
            .filter(|request| request.path.contains(fragment) && request.authorization.is_some())
            .map(|request| request.authorization.unwrap())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert!(bearer("/saas-fabric/").is_disjoint(&bearer("/saas-fabric-control-plane-ui/")));
}

#[tokio::test]
async fn a_distribution_registry_answers_basic_on_its_own_origin_for_its_repositories() {
    let fake = FakeRegistry::start().await;
    fake.challenge(Challenge::Basic);
    fake.expect_credential(USERNAME, SECRET);
    fake.publish("team/app", "1.0.0", "abc");
    fake.publish("team/other", "1.0.0", "abc");
    let registry = distribution(&fake, &["registry.example/team/app"]);

    let tags = registry.tags("registry.example/team/app").await.unwrap();
    assert_eq!(tags, vec!["1.0.0".to_owned()]);
    let anonymous = registry.tags("registry.example/team/other").await;
    assert!(
        matches!(anonymous, Err(RegistryError::Refused { .. })),
        "{anonymous:?}"
    );

    for request in fake.requests() {
        if request.path.starts_with("/v2/team/other/") {
            assert_eq!(request.authorization, None, "{request:?}");
        }
    }
    assert!(fake
        .requests()
        .iter()
        .any(|request| request.authorization == Some(basic(USERNAME, SECRET))));
}

#[tokio::test]
async fn only_a_distribution_registry_answers_basic() {
    let fake = FakeRegistry::start().await;
    fake.challenge(Challenge::Basic);
    fake.expect_credential(USERNAME, SECRET);
    fake.publish(RUNTIME, "0.3.0", "abc");
    let registry = deployment(&fake, &[RUNTIME]);

    let failure = registry.tags(RUNTIME).await.expect_err("Basic is not answered");

    assert!(matches!(failure, RegistryError::Refused { .. }), "{failure:?}");
    assert!(fake
        .requests()
        .iter()
        .all(|request| request.authorization.is_none()));
}

#[tokio::test]
async fn a_refused_credential_is_marked_and_not_presented_again() {
    let fake = FakeRegistry::start_with_realm().await;
    fake.realm_refuses();
    fake.publish(RUNTIME, "0.3.0", "abc");
    fake.publish(CONSOLE, "0.3.0", "abc");
    let registry = deployment(&fake, &[RUNTIME]);
    assert!(!registry.credential_refused());

    let failure = registry.resolve(RUNTIME, "0.3.0").await.expect_err("refused");
    assert!(matches!(failure, RegistryError::Denied { .. }), "{failure:?}");
    assert!(registry.credential_refused());
    assert_no_secret(&failure);

    let (realm, registry_asked) = (fake.realm_requests().len(), fake.requests().len());
    let again = registry
        .resolve(RUNTIME, "0.3.0")
        .await
        .expect_err("still refused");
    assert!(matches!(again, RegistryError::Denied { .. }), "{again:?}");
    assert_no_secret(&again);
    assert_eq!(
        fake.realm_requests().len(),
        realm,
        "the realm was not asked again"
    );
    assert_eq!(fake.requests().len(), registry_asked, "nor was the registry");

    registry
        .resolve(CONSOLE, "0.3.0")
        .await
        .unwrap()
        .expect("anonymous reads go on");
    assert!(
        !deployment(&fake, &[RUNTIME]).credential_refused(),
        "a new client starts unmarked"
    );
}

#[tokio::test]
async fn a_basic_credential_the_registry_refuses_is_marked_too() {
    let fake = FakeRegistry::start().await;
    fake.challenge(Challenge::Basic);
    fake.expect_credential(USERNAME, "another secret");
    fake.publish("team/app", "1.0.0", "abc");
    let registry = distribution(&fake, &["registry.example/team/app"]);

    let failure = registry
        .tags("registry.example/team/app")
        .await
        .expect_err("refused");

    assert!(matches!(failure, RegistryError::Denied { .. }), "{failure:?}");
    assert!(registry.credential_refused());
    assert_no_secret(&failure);
}

#[tokio::test]
async fn a_basic_credential_never_follows_a_blob_to_its_cdn() {
    let fake = FakeRegistry::start_with_cdn().await;
    fake.cdn_by_name();
    fake.cdn_redirects_twice();
    fake.challenge(Challenge::Basic);
    fake.expect_credential(USERNAME, SECRET);
    fake.publish("team/app", "1.0.0", "abc");
    let registry = distribution(&fake, &["registry.example/team/app"]);

    let resolved = registry
        .resolve("registry.example/team/app", "1.0.0")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(resolved.provenance, Provenance::Agreed("abc".to_owned()));
    let asked = fake
        .requests()
        .into_iter()
        .filter(|request| request.path.contains("/blobs/"))
        .collect::<Vec<_>>();
    assert!(
        asked
            .iter()
            .any(|request| request.authorization == Some(basic(USERNAME, SECRET))),
        "the registry's own origin is asked with it: {asked:?}"
    );
    let served = fake.cdn_requests();
    assert_eq!(served.len(), 2, "{served:?}");
    assert!(
        served.iter().all(|request| request.authorization.is_none()),
        "no credential crosses an origin, on any hop: {served:?}"
    );
}

#[tokio::test]
async fn a_refusal_marks_every_client_built_with_the_same_credential() {
    let fake = FakeRegistry::start_with_realm().await;
    fake.realm_refuses();
    fake.publish(RUNTIME, "0.3.0", "abc");
    let mark = Arc::new(AtomicBool::new(false));
    let build = |repositories: &[&str]| {
        let settings = RegistrySettings::deployment(&fake.base_url, HOST)
            .unwrap()
            .serve_from(&fake.base_url)
            .unwrap()
            .with_credential(credential(repositories).sharing_refusal(Arc::clone(&mark)));
        OciRegistry::with_settings(settings, 5).unwrap()
    };
    let live = build(&[RUNTIME]);
    let proving = build(&[RUNTIME, CONSOLE]);

    let failure = proving.resolve(RUNTIME, "0.3.0").await.expect_err("refused");
    assert!(matches!(failure, RegistryError::Denied { .. }), "{failure:?}");

    assert!(
        live.credential_refused(),
        "the client discovery reads through is marked too"
    );
    let asked = fake.realm_requests().len();
    assert!(matches!(
        live.resolve(RUNTIME, "0.3.0").await,
        Err(RegistryError::Denied { .. })
    ));
    assert_eq!(fake.realm_requests().len(), asked, "and presents it no more");
    assert!(
        build(&[]).credential_refused(),
        "a client rebuilt with the same credential keeps the mark"
    );
}
