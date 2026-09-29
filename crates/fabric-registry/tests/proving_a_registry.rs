//! What proving a registry, and a repository through it, answers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_platform_management::RegistryError;
use fabric_registry::{Credential, OciRegistry, Readability, RealmRule, RegistrySecret, RegistrySettings};
use support::{basic, localhost, Challenge, FakeRegistry, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const SECRET: &str = "a-long-lived-token";

fn anonymous(fake: &FakeRegistry) -> OciRegistry {
    OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5).unwrap()
}

fn credentialed(fake: &FakeRegistry, repositories: &[&str]) -> OciRegistry {
    let credential =
        Credential::new("brett", RegistrySecret::new(SECRET), repositories.iter().copied()).unwrap();
    let settings = RegistrySettings::deployment(&fake.base_url, HOST)
        .unwrap()
        .serve_from(&fake.base_url)
        .unwrap()
        .with_credential(credential);
    OciRegistry::with_settings(settings, 5).unwrap()
}

fn origin_of(url: &str) -> String {
    reqwest::Url::parse(url).unwrap().origin().ascii_serialization()
}

#[tokio::test]
async fn a_registry_is_proven_through_its_challenge_and_names_its_realm() {
    let fake = FakeRegistry::start_with_realm().await;
    fake.expect_credential("brett", SECRET);

    let proof = credentialed(&fake, &[]).prove().await.unwrap();

    assert_eq!(proof.realm_origin(), Some(origin_of(&fake.realm()).as_str()));
    let asked = fake.realm_requests();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(
        asked[0].authorization,
        Some(basic("brett", SECRET)),
        "proving presents it"
    );
    assert!(
        !asked[0].path.contains("scope="),
        "proving /v2/ asks no scope: {}",
        asked[0].path
    );
}

#[tokio::test]
async fn a_registry_that_asks_for_basic_or_nothing_names_no_realm() {
    let open = FakeRegistry::start().await;
    open.challenge(Challenge::None);
    assert_eq!(anonymous(&open).prove().await.unwrap().realm_origin(), None);

    let basic_only = FakeRegistry::start().await;
    basic_only.challenge(Challenge::Basic);
    basic_only.expect_credential("brett", SECRET);
    let credential = Credential::new(
        "brett",
        RegistrySecret::new(SECRET),
        ["registry.example/team/app"],
    )
    .unwrap();
    let settings = RegistrySettings::distribution("https://registry.example", RealmRule::FollowChallenge)
        .unwrap()
        .serve_from(&localhost(&basic_only.base_url))
        .unwrap()
        .treat_loopback_as_public()
        .with_credential(credential);

    let proof = OciRegistry::with_settings(settings, 5)
        .unwrap()
        .prove()
        .await
        .unwrap();

    assert_eq!(proof.realm_origin(), None);
}

#[tokio::test]
async fn a_realm_refusing_the_credential_while_proving_is_denied_and_remembered() {
    let fake = FakeRegistry::start_with_realm().await;
    fake.realm_refuses();
    let registry = credentialed(&fake, &[]);

    let failure = registry.prove().await.expect_err("denied");
    assert!(matches!(failure, RegistryError::Denied { .. }), "{failure:?}");

    let asked = fake.realm_requests().len();
    assert!(matches!(
        registry.prove().await,
        Err(RegistryError::Denied { .. })
    ));
    assert_eq!(fake.realm_requests().len(), asked, "not presented again");
}

#[tokio::test]
async fn an_anonymous_proof_ends_at_the_challenge_and_asks_the_realm_nothing() {
    // GHCR's realm refuses an anonymous request for no scope outright, while
    // granting every public repository: asking it would refuse a registry
    // anyone can read.
    let fake = FakeRegistry::start_with_realm().await;
    fake.realm_declines_anonymous_unscoped();

    let proof = anonymous(&fake).prove().await.unwrap();

    assert_eq!(proof.realm_origin(), Some(origin_of(&fake.realm()).as_str()));
    assert!(fake.realm_requests().is_empty(), "{:?}", fake.realm_requests());
}

#[tokio::test]
async fn ghcr_is_proven_anonymously_by_a_challenge_naming_its_own_realm() {
    let fake = FakeRegistry::start().await;
    fake.challenge(Challenge::Bearer {
        realm: "https://ghcr.io/token".to_owned(),
        service: "ghcr.io".to_owned(),
    });
    let settings = RegistrySettings::ghcr()
        .serve_from(&localhost(&fake.base_url))
        .unwrap()
        .treat_loopback_as_public();

    let proof = OciRegistry::with_settings(settings, 5)
        .unwrap()
        .prove()
        .await
        .unwrap();

    assert_eq!(proof.realm_origin(), Some("https://ghcr.io"));
    assert_eq!(fake.mints(), 0);
}

#[tokio::test]
async fn a_repository_is_proven_by_its_tag_listing() {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let registry = anonymous(&fake);

    assert_eq!(
        registry.prove_repository(RUNTIME).await.unwrap(),
        Readability::Readable(())
    );
    assert_eq!(
        registry.version_tags(RUNTIME).await.unwrap(),
        Readability::Readable(vec!["0.3.0".to_owned()])
    );
}

#[tokio::test]
async fn a_repository_answering_401_403_or_404_is_not_readable_through_this_registry() {
    for status in [401, 403, 404] {
        let fake = FakeRegistry::start().await;
        fake.tags_answer(status);
        let registry = anonymous(&fake);

        assert_eq!(
            registry.prove_repository(RUNTIME).await.unwrap(),
            Readability::NotReadable,
            "{status}"
        );
        assert_eq!(
            registry.version_tags(RUNTIME).await.unwrap(),
            Readability::NotReadable,
            "{status}"
        );
    }
}

#[tokio::test]
async fn a_realm_declining_one_repository_leaves_the_credential_unmarked() {
    let fake = FakeRegistry::start_with_realm().await;
    fake.expect_credential("brett", SECRET);
    fake.realm_declines_scopes();
    fake.publish(RUNTIME, "0.3.0", "abc");
    let registry = credentialed(&fake, &[RUNTIME]);

    assert_eq!(
        registry.prove_repository(RUNTIME).await.unwrap(),
        Readability::NotReadable
    );
    assert!(
        !registry.credential_refused(),
        "a scope declined is not the credential refused"
    );
}

#[tokio::test]
async fn a_realm_changed_while_proving_a_repository_is_an_error_not_unreadable() {
    let fake = FakeRegistry::start_with_realm().await;
    fake.publish(RUNTIME, "0.3.0", "abc");
    let registry = anonymous(&fake);
    registry.prove().await.unwrap();
    let first = fake.realm();

    let moved = format!("{}/elsewhere", localhost(&fake.base_url));
    fake.challenge(Challenge::Bearer {
        realm: moved.clone(),
        service: HOST.to_owned(),
    });

    match registry.prove_repository(RUNTIME).await {
        Err(RegistryError::Refused { detail }) => {
            assert!(detail.contains(&origin_of(&first)), "{detail}");
            assert!(detail.contains(&origin_of(&moved)), "{detail}");
        }
        other => panic!("expected the realm change, got {other:?}"),
    }
}

#[tokio::test]
async fn a_repository_whose_credential_is_refused_is_denied_not_unreadable() {
    let fake = FakeRegistry::start_with_realm().await;
    fake.realm_refuses();
    fake.publish(RUNTIME, "0.3.0", "abc");

    let failure = credentialed(&fake, &[RUNTIME]).prove_repository(RUNTIME).await;

    assert!(
        matches!(failure, Err(RegistryError::Denied { .. })),
        "{failure:?}"
    );
}
