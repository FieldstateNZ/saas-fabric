//! The Keycloak adapter, against something that speaks HTTP.

// A test's helpers assert their own preconditions; `unwrap` there is the
// assertion, not a hole. Clippy's `allow-unwrap-in-tests` only covers
// `#[test]` functions, so an integration test file states it once here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::{Arc, Mutex};

use fabric_client_model::{ClientProtocol, OidcClient, OidcClientId, PkceMethod, RealmName, RoleName};
use fabric_client_model::{RedirectStrategy, RedirectStrategyKind, RedirectUri};
use fabric_keycloak::KeycloakIdentityProvider;
use fabric_reconciliation::{IdentityProvider, ProviderError};
use support::{config_for_tests, FakeKeycloak, RecordedRequest};

/// The audience this test suite's provider is configured with. The value
/// itself is never under test here — see `keycloak_adapter_identity.rs` for
/// that — so one fixed string is all any of these tests need.
const AUDIENCE: &str = "saas-fabric-data-api";

fn realm() -> RealmName {
    RealmName::try_new("acme").unwrap()
}

fn web_client() -> OidcClient {
    OidcClient {
        id: OidcClientId::try_new("web").unwrap(),
        protocol: ClientProtocol::Oidc,
        pkce: PkceMethod::S256,
        redirect: RedirectStrategy::try_new(
            RedirectStrategyKind::ClaimedHttps,
            vec![RedirectUri::try_new("https://www.example.com/callback").unwrap()],
        )
        .unwrap(),
    }
}

/// Builds a provider pointed at a fake Keycloak.
fn provider(keycloak: &FakeKeycloak) -> KeycloakIdentityProvider {
    let config = config_for_tests(&keycloak.base_url, AUDIENCE);

    KeycloakIdentityProvider::new(&config, "an-operators-token").expect("the provider must build")
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
async fn every_admin_request_carries_the_operators_own_bearer() {
    // The platform holds no credential for Keycloak. What goes out is the
    // token the operator presented, unchanged — an adapter that substituted
    // anything of its own would be the standing authority ADR 0012 removed.
    let keycloak = populated().await;
    provider(&keycloak).observe_realm(&realm()).await.unwrap();

    let requests = keycloak.admin_requests();
    assert!(!requests.is_empty(), "no admin request was made");
    assert!(
        requests
            .iter()
            .all(|request| request.bearer.as_deref() == Some("an-operators-token")),
        "an admin request went out with something other than the operator's bearer"
    );
}

#[tokio::test]
async fn no_credential_is_ever_exchanged_for_a_token() {
    // There is nothing to exchange. A token endpoint call here would mean the
    // adapter had acquired an authority of its own.
    let keycloak = populated().await;
    let provider = provider(&keycloak);

    provider.observe_realm(&realm()).await.unwrap();
    provider.observe_realm(&realm()).await.unwrap();

    assert_eq!(
        keycloak.count("POST", "/realms/master/protocol/openid-connect/token"),
        0,
        "the adapter tried to mint a token of its own"
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
async fn a_refusal_is_reported_rather_than_retried() {
    // There used to be a retry here, for a real reason found against real
    // Keycloak: creating a realm grants the creator that realm's admin roles,
    // into tokens minted *afterwards*, so the pass that created a realm was
    // then refused inside it with a token that was valid and simply too old.
    // A service account could mint a fresh one and carry on.
    //
    // A borrowed token cannot be re-minted, so the retry is gone and a refusal
    // is reported. The consequence moved onto the operator instead: their
    // authority has to already cover realms that do not exist yet, which
    // master-realm `admin` does and `create-realm` alone does not (ADR 0012).
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (403, "{}".to_owned()))).await;

    let outcome = provider(&keycloak)
        .create_realm_role(&realm(), &RoleName::try_new("Client Realm User").unwrap())
        .await;

    assert_eq!(outcome, Err(ProviderError::NotPermitted));
    assert_eq!(
        keycloak.admin_requests().len(),
        1,
        "one attempt: there is no second authority to try"
    );
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
    // Port 1 is reserved and nothing listens on it, so the connect is refused
    // rather than hanging until a timeout.
    let config = config_for_tests("http://127.0.0.1:1", AUDIENCE);
    let provider = KeycloakIdentityProvider::new(&config, "an-operators-token").unwrap();

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
