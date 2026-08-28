//! The Keycloak adapter, against something that speaks HTTP.

// A test's helpers assert their own preconditions; `unwrap` there is the
// assertion, not a hole. Clippy's `allow-unwrap-in-tests` only covers
// `#[test]` functions, so an integration test file states it once here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use fabric_client_model::{ClientProtocol, OidcClient, OidcClientId, RealmName, RedirectUri, RoleName};
use fabric_core::Clock;
use fabric_keycloak::{AdminCredential, KeycloakConfig, KeycloakIdentityProvider};
use fabric_reconciliation::{IdentityProvider, ProviderError};
use support::{FakeKeycloak, RecordedRequest};

/// A clock the token cache can measure against.
struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_700_000_000
    }
}

fn realm() -> RealmName {
    RealmName::try_new("acme").unwrap()
}

fn web_client() -> OidcClient {
    OidcClient {
        id: OidcClientId::try_new("web").unwrap(),
        protocol: ClientProtocol::Oidc,
        redirect_uris: vec![RedirectUri::try_new("https://www.example.com/callback").unwrap()],
    }
}

/// Builds a provider pointed at a fake Keycloak.
fn provider(keycloak: &FakeKeycloak) -> KeycloakIdentityProvider {
    let config = KeycloakConfig {
        base_url: keycloak.base_url.clone(),
        ..KeycloakConfig::default()
    };

    KeycloakIdentityProvider::new(&config, AdminCredential::new("test-secret"), Arc::new(TestClock))
        .expect("the provider must build")
}

/// A Keycloak holding one realm with two roles and one application client.
async fn populated() -> FakeKeycloak {
    FakeKeycloak::start(Arc::new(|request: &RecordedRequest| match request.path.as_str() {
        path if path.starts_with("/admin/realms/acme/roles") => (
            200,
            r#"[{"name":"Client Realm Administrator"},{"name":"Client Realm User"},{"name":"offline_access"}]"#
                .to_owned(),
        ),
        path if path.starts_with("/admin/realms/acme/clients") => (
            200,
            r#"[{"id":"uuid-1","clientId":"web","redirectUris":["https://www.example.com/callback"],"publicClient":true}]"#
                .to_owned(),
        ),
        "/admin/realms/acme" => (200, r#"{"displayName":"Acme"}"#.to_owned()),
        _ => (404, "{}".to_owned()),
    }))
    .await
}

#[tokio::test]
async fn an_absent_realm_is_reported_as_absent_rather_than_as_a_failure() {
    // The branch the whole reconciler hangs off: 404 means "create it", any
    // other failure means "do not touch it".
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (404, "{}".to_owned()))).await;

    assert_eq!(provider(&keycloak).observe_realm(&realm()).await, Ok(None));
}

#[tokio::test]
async fn an_existing_realm_is_read_with_its_roles_and_clients() {
    let keycloak = populated().await;

    let observed = provider(&keycloak)
        .observe_realm(&realm())
        .await
        .unwrap()
        .expect("the realm exists");

    assert_eq!(observed.display_name, "Acme");
    assert_eq!(observed.roles.len(), 3, "roles Keycloak owns are reported too");
    assert!(observed
        .roles
        .contains(&RoleName::try_new("Client Realm User").unwrap()));
    assert_eq!(observed.clients.len(), 1);
    assert!(observed.clients[&web_client().id].public);
}

#[tokio::test]
async fn every_admin_request_carries_a_bearer_token() {
    // The token exchange is not optional and not cached across processes; an
    // adapter that skipped it would 401 on the first real Keycloak.
    let keycloak = populated().await;
    provider(&keycloak).observe_realm(&realm()).await.unwrap();

    assert!(keycloak.count("POST", "/realms/master/protocol/openid-connect/token") >= 1);
    assert!(
        keycloak.admin_requests().iter().all(|request| request.authorised),
        "an admin request went out unauthenticated"
    );
}

#[tokio::test]
async fn the_token_is_reused_across_calls_in_one_sweep() {
    let keycloak = populated().await;
    let provider = provider(&keycloak);

    provider.observe_realm(&realm()).await.unwrap();
    provider.observe_realm(&realm()).await.unwrap();

    assert_eq!(
        keycloak.count("POST", "/realms/master/protocol/openid-connect/token"),
        1,
        "the credential was exchanged more than once for a token that is still good"
    );
}

#[tokio::test]
async fn creating_a_realm_sends_the_name_and_enables_it() {
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (201, String::new()))).await;

    provider(&keycloak).create_realm(&realm(), "Acme").await.unwrap();

    let request = keycloak
        .admin_requests()
        .into_iter()
        .find(|request| request.method == "POST" && request.path == "/admin/realms")
        .expect("the realm must be created at the realms collection");

    assert!(request.body.contains(r#""realm":"acme""#));
    assert!(request.body.contains(r#""displayName":"Acme""#));
    assert!(request.body.contains(r#""enabled":true"#));
}

#[tokio::test]
async fn creating_something_that_already_exists_succeeds() {
    // The port requires it: Keycloak creates roles of its own with every
    // realm, so a 409 must not fail a sweep.
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (409, "{}".to_owned()))).await;
    let provider = provider(&keycloak);

    assert_eq!(provider.create_realm(&realm(), "Acme").await, Ok(()));
    assert_eq!(
        provider
            .create_realm_role(&realm(), &RoleName::try_new("Client Realm User").unwrap())
            .await,
        Ok(())
    );
    assert_eq!(provider.create_oidc_client(&realm(), &web_client()).await, Ok(()));
}

#[tokio::test]
async fn a_declared_client_is_written_as_a_public_authorisation_code_client() {
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (201, String::new()))).await;

    provider(&keycloak)
        .create_oidc_client(&realm(), &web_client())
        .await
        .unwrap();

    let request = keycloak
        .admin_requests()
        .into_iter()
        .find(|request| request.method == "POST")
        .expect("the client must be created");

    assert!(request.body.contains(r#""clientId":"web""#));
    assert!(request.body.contains(r#""publicClient":true"#));
    assert!(request.body.contains(r#""standardFlowEnabled":true"#));
    assert!(request.body.contains("https://www.example.com/callback"));
    assert!(
        !request.body.contains("secret"),
        "a declared client must never carry a secret"
    );
}

#[tokio::test]
async fn updating_a_client_addresses_keycloaks_internal_id_not_the_client_id() {
    // Sending one where the other belongs produces a 404 that reads like the
    // client does not exist.
    let keycloak = FakeKeycloak::start(Arc::new(|request: &RecordedRequest| {
        if request.method == "GET" {
            (
                200,
                r#"[{"id":"uuid-1","clientId":"web","redirectUris":[],"publicClient":true}]"#.to_owned(),
            )
        } else {
            (204, String::new())
        }
    }))
    .await;

    provider(&keycloak)
        .update_oidc_client(&realm(), &web_client())
        .await
        .unwrap();

    assert_eq!(keycloak.count("PUT", "/admin/realms/acme/clients/uuid-1"), 1);
}

#[tokio::test]
async fn a_client_that_vanished_between_observation_and_update_is_created() {
    let created = Arc::new(Mutex::new(0_usize));
    let counter = Arc::clone(&created);

    let keycloak = FakeKeycloak::start(Arc::new(move |request: &RecordedRequest| {
        if request.method == "GET" {
            return (200, "[]".to_owned());
        }
        *counter.lock().unwrap() += 1;
        (201, String::new())
    }))
    .await;

    provider(&keycloak)
        .update_oidc_client(&realm(), &web_client())
        .await
        .unwrap();

    assert_eq!(*created.lock().unwrap(), 1);
}

#[tokio::test]
async fn a_refused_credential_is_reported_as_not_permitted() {
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (403, "{}".to_owned()))).await;

    assert_eq!(
        provider(&keycloak).observe_realm(&realm()).await,
        Err(ProviderError::NotPermitted)
    );
}

#[tokio::test]
async fn keycloaks_own_error_body_never_reaches_the_error() {
    // The body is the thing an operator's browser must not render, and the
    // adapter is where it is dropped.
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| {
        (
            500,
            r#"{"error":"unknown_error","error_description":"realm acme master password hash"}"#.to_owned(),
        )
    }))
    .await;

    let error = provider(&keycloak).observe_realm(&realm()).await.unwrap_err();
    let message = error.to_string();

    assert!(!message.contains("password"));
    assert!(!message.contains("unknown_error"));
    assert!(message.contains("500"));
}

#[tokio::test]
async fn an_unreachable_keycloak_is_reported_as_unavailable_and_transient() {
    let config = KeycloakConfig {
        // Port 1 is reserved and nothing listens on it, so the connect is
        // refused rather than hanging until a timeout.
        base_url: "http://127.0.0.1:1".to_owned(),
        ..KeycloakConfig::default()
    };
    let provider =
        KeycloakIdentityProvider::new(&config, AdminCredential::new("s"), Arc::new(TestClock)).unwrap();

    let error = provider.observe_realm(&realm()).await.unwrap_err();

    assert!(error.is_transient(), "{error}");
}

#[tokio::test]
async fn the_description_names_the_endpoint_and_no_credential() {
    let keycloak = populated().await;

    let description = provider(&keycloak).describe();

    assert!(description.contains(&keycloak.base_url));
    assert!(!description.contains("test-secret"));
}
