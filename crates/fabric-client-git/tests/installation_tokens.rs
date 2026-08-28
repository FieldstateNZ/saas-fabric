//! The GitHub App posture: minting a short-lived token from a private key.
//!
//! This is the production credential path (specification §3), so it is tested
//! against a socket rather than argued about. What it pins is that the adapter
//! signs an assertion, exchanges it, presents the *minted* token to the
//! contents API rather than the key, and does not re-mint per request.

// A test's helpers assert their own preconditions; `unwrap` there is the
// assertion, not a hole.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Arc;
use std::time::Instant;

use fabric_client_git::{GitAuthConfig, GitClientRepository, GitCredential, GitRepositoryConfig};
use fabric_client_model::ClientId;
use fabric_control_plane::{ClientRepository, RepositoryError};
use fabric_core::Clock;
use support::{FakeGitHost, MINTED_TOKEN};

/// A throwaway RSA key, generated for this test and used nowhere else.
///
/// Committed deliberately: it authenticates nothing, and a test that generated
/// one at runtime would need a key generator in the dev-dependency graph to
/// prove a property about signing.
const TEST_KEY: &str = include_str!("support/test-app-key.pem");

/// Where the fixture client's document lives.
const ACME_PATH: &str = "clients/acme/client.yaml";

/// A minimal valid client document.
const ACME: &str = r"apiVersion: fabric.fieldstate.nz/v1
kind: Client
metadata:
  name: acme
spec:
  displayName: Acme
  identity:
    realm: acme
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients: []
";

/// A clock the token cache measures against.
struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_700_000_000
    }
}

/// A repository authenticating as a GitHub App installation.
fn app_repository(host: &FakeGitHost, private_key: &str) -> GitClientRepository {
    let config = GitRepositoryConfig {
        api_base_url: host.base_url.clone(),
        auth: GitAuthConfig::GithubApp {
            app_id: "123456".to_owned(),
            installation_id: "7891011".to_owned(),
            private_key_ref: "git/test-app-key".to_owned(),
        },
        ..GitRepositoryConfig::default()
    };

    GitClientRepository::new(
        &config,
        GitCredential::app("123456", "7891011", private_key),
        Arc::new(TestClock),
    )
    .expect("the repository must build")
}

fn client() -> ClientId {
    ClientId::try_new("acme").unwrap()
}

#[tokio::test]
async fn the_key_is_exchanged_for_a_token_at_the_installation_endpoint() {
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;

    app_repository(&host, TEST_KEY).get(&client()).await.unwrap();

    let mint = host
        .requests()
        .into_iter()
        .find(|request| request.path.contains("/access_tokens"))
        .expect("the adapter must mint an installation token");

    assert_eq!(mint.method, "POST");
    assert_eq!(mint.path, "/app/installations/7891011/access_tokens");
}

#[tokio::test]
async fn the_assertion_is_a_signed_jwt_and_not_the_key() {
    // The failure this closes: presenting the PEM as a bearer, which a host
    // would reject and a log might record.
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;

    app_repository(&host, TEST_KEY).get(&client()).await.unwrap();

    let mint = host
        .requests()
        .into_iter()
        .find(|request| request.path.contains("/access_tokens"))
        .unwrap();
    let assertion = mint.authorization.unwrap();

    assert!(assertion.starts_with("Bearer "));
    assert!(
        !assertion.contains("BEGIN"),
        "the private key was sent as a bearer"
    );
    // Three dot-separated segments: header, claims, signature.
    assert_eq!(assertion.trim_start_matches("Bearer ").split('.').count(), 3);
}

#[tokio::test]
async fn the_contents_api_is_called_with_the_minted_token() {
    // The key authenticates the *mint*; everything after it uses the token
    // that mint returned.
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;

    app_repository(&host, TEST_KEY).get(&client()).await.unwrap();

    let contents = host
        .requests()
        .into_iter()
        .find(|request| request.path.contains("/contents/"))
        .expect("the adapter must read the document");

    assert_eq!(
        contents.authorization.as_deref(),
        Some(format!("Bearer {MINTED_TOKEN}").as_str())
    );
}

#[tokio::test]
async fn the_token_is_minted_once_and_reused() {
    // A mint per request would be three round trips per client per sweep, and
    // would hit the host's rate limit on a platform with real client counts.
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;
    let repository = app_repository(&host, TEST_KEY);

    repository.get(&client()).await.unwrap();
    repository.get(&client()).await.unwrap();
    repository.list().await.unwrap();

    let mints = host
        .requests()
        .iter()
        .filter(|request| request.path.contains("/access_tokens"))
        .count();

    assert_eq!(mints, 1, "the key was exchanged more than once for a live token");
}

#[tokio::test]
async fn a_key_that_is_not_a_private_key_is_refused_rather_than_sent() {
    // An unparseable key must fail before anything reaches the network: the
    // alternative is a request carrying whatever the value happened to be.
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;

    let error = app_repository(&host, "not a pem")
        .get(&client())
        .await
        .unwrap_err();

    assert!(matches!(error, RepositoryError::NotPermitted), "{error}");
    assert!(
        host.requests().is_empty(),
        "a request went out with an unusable key"
    );
}

#[tokio::test]
async fn a_failure_never_carries_the_key() {
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;

    let error = app_repository(&host, "-----BEGIN PRIVATE KEY-----\nnotreal\n")
        .get(&client())
        .await
        .unwrap_err()
        .to_string();

    assert!(!error.contains("BEGIN"));
    assert!(!error.contains("notreal"));
}
