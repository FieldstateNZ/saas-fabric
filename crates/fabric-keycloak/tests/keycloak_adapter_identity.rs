//! The Keycloak adapter's identity contract: PKCE, the audience mapper, the
//! post-logout redirect set, and the drift an unparseable redirect URI is.
//!
//! Split from `keycloak_adapter.rs` — ADR 0019's slice 4 — so that file stays
//! about the realm/role/client CRUD the adapter already did, and this one is
//! about what it now writes and reads for a client's identity.

// A test's helpers assert their own preconditions; `unwrap` there is the
// assertion, not a hole. See `keycloak_adapter.rs` for the same allowance.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;

use fabric_client_model::{AppScheme, ClientProtocol, OidcClient, OidcClientId, PkceMethod, RealmName};
use fabric_client_model::{RedirectStrategy, RedirectStrategyKind, RedirectUri};
use fabric_keycloak::{KeycloakConfig, KeycloakIdentityProvider};
use fabric_reconciliation::{IdentityProvider, ProviderError};
use support::{FakeKeycloak, RecordedRequest};

/// The audience this test suite's provider is configured with.
const AUDIENCE: &str = "saas-fabric-data-api";

fn realm() -> RealmName {
    RealmName::try_new("acme").unwrap()
}

/// A production web client: `claimedHttps`, one callback.
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

/// A native client: `development`, a portless loopback callback — the shape
/// RFC 8252 §7.3 recommends for a desktop shell.
fn native_client() -> OidcClient {
    OidcClient {
        id: OidcClientId::try_new("slipway").unwrap(),
        protocol: ClientProtocol::Oidc,
        pkce: PkceMethod::S256,
        redirect: RedirectStrategy::try_new(
            RedirectStrategyKind::Development,
            vec![RedirectUri::try_new("http://127.0.0.1/callback").unwrap()],
        )
        .unwrap(),
    }
}

/// A client declaring the `customScheme` strategy.
///
/// Model validation refuses this before a document is ever written (ADR 0019
/// §3), so it can only be built directly, bypassing that validation — exactly
/// what proving the adapter's *own* refusal requires: this must never reach
/// this crate any other way, but if it does, the adapter has to say no rather
/// than write a client with no callbacks.
fn custom_scheme_client() -> OidcClient {
    OidcClient {
        id: OidcClientId::try_new("desktop").unwrap(),
        protocol: ClientProtocol::Oidc,
        pkce: PkceMethod::S256,
        redirect: RedirectStrategy::try_new(
            RedirectStrategyKind::CustomScheme(AppScheme::try_new("nz.fieldstate.slipway").unwrap()),
            vec![RedirectUri::try_new("nz.fieldstate.slipway:/callback").unwrap()],
        )
        .unwrap(),
    }
}

/// Builds a provider pointed at a fake Keycloak, configured with
/// [`AUDIENCE`].
fn provider(keycloak: &FakeKeycloak) -> KeycloakIdentityProvider {
    let config = KeycloakConfig {
        base_url: keycloak.base_url.clone(),
        audience: AUDIENCE.to_owned(),
        ..KeycloakConfig::default()
    };

    KeycloakIdentityProvider::new(&config, "an-operators-token").expect("the provider must build")
}

/// Finds the one `POST` this test's fake received.
fn created_client_body(keycloak: &FakeKeycloak) -> String {
    keycloak
        .admin_requests()
        .into_iter()
        .find(|request| request.method == "POST")
        .expect("the client must be created")
        .body
}

#[tokio::test]
async fn a_declared_client_is_written_with_the_s256_challenge_method() {
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (201, String::new()))).await;

    provider(&keycloak)
        .create_oidc_client(&realm(), &web_client())
        .await
        .unwrap();

    assert!(created_client_body(&keycloak).contains(r#""pkce.code.challenge.method":"S256""#));
}

#[tokio::test]
async fn a_declared_client_is_written_with_the_platform_audience_mapper() {
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (201, String::new()))).await;

    provider(&keycloak)
        .create_oidc_client(&realm(), &web_client())
        .await
        .unwrap();

    let body = created_client_body(&keycloak);

    assert!(body.contains(r#""oidc-audience-mapper""#));
    assert!(body.contains(&format!(r#""included.custom.audience":"{AUDIENCE}""#)));
}

#[tokio::test]
async fn a_declared_client_is_written_with_the_registered_uris_as_its_post_logout_set() {
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (201, String::new()))).await;

    provider(&keycloak)
        .create_oidc_client(&realm(), &web_client())
        .await
        .unwrap();

    assert!(created_client_body(&keycloak).contains(r#""post.logout.redirect.uris":"+""#));
}

#[tokio::test]
async fn an_unmodellable_redirect_uri_is_counted_rather_than_dropped() {
    // `http://evil.example.com/steal` is plain HTTP on a public host, which
    // this model refuses to parse (D10) — the same example ADR 0019 §6 and
    // the test matrix's D13 use for "an unparseable URI added by hand".
    let keycloak = FakeKeycloak::start(Arc::new(|request: &RecordedRequest| match request.path.as_str() {
        path if path.starts_with("/admin/realms/acme/roles") => (200, "[]".to_owned()),
        path if path.starts_with("/admin/realms/acme/clients") => (
            200,
            r#"[{"id":"uuid-1","clientId":"web","redirectUris":["https://www.example.com/callback","http://evil.example.com/steal"],"publicClient":true}]"#
                .to_owned(),
        ),
        "/admin/realms/acme" => (200, r#"{"displayName":"Acme"}"#.to_owned()),
        _ => (404, "{}".to_owned()),
    }))
    .await;

    let observed = provider(&keycloak)
        .observe_realm(&realm())
        .await
        .unwrap()
        .expect("the realm exists");

    let client = observed
        .clients
        .get(&web_client().id)
        .expect("the client is reported");

    assert_eq!(
        client.redirect_uris.len(),
        1,
        "the unparseable entry must never appear in the parsed set"
    );
    assert_eq!(
        client.unmodellable_redirect_uris, 1,
        "it must be counted instead of silently dropped"
    );
}

#[tokio::test]
async fn two_audience_mappers_are_observed_as_no_single_audience() {
    // A second `oidc-audience-mapper` added out of band — this adapter never
    // writes more than one, but Keycloak has no opinion about that, and it
    // returns mappers in its own order rather than write order. "First"
    // would silently pick whichever one Keycloak happens to list first;
    // "exactly one" is the only reading that cannot hide a second mapper
    // from every sweep (see `audience_mapper`'s rustdoc).
    let keycloak = FakeKeycloak::start(Arc::new(|request: &RecordedRequest| match request.path.as_str() {
        path if path.starts_with("/admin/realms/acme/roles") => (200, "[]".to_owned()),
        path if path.starts_with("/admin/realms/acme/clients") => (
            200,
            format!(
                r#"[{{"id":"uuid-1","clientId":"web","redirectUris":[],"publicClient":true,"protocolMappers":[
                    {{"protocolMapper":"oidc-audience-mapper","config":{{"included.custom.audience":"{AUDIENCE}"}}}},
                    {{"protocolMapper":"oidc-audience-mapper","config":{{"included.custom.audience":"an-out-of-band-audience"}}}}
                ]}}]"#
            ),
        ),
        "/admin/realms/acme" => (200, r#"{"displayName":"Acme"}"#.to_owned()),
        _ => (404, "{}".to_owned()),
    }))
    .await;

    let observed = provider(&keycloak)
        .observe_realm(&realm())
        .await
        .unwrap()
        .expect("the realm exists");

    let client = observed
        .clients
        .get(&web_client().id)
        .expect("the client is reported");

    assert_eq!(
        client.audience_mapper, None,
        "two audience mappers must read as no single audience, not as either one of them"
    );
}

#[tokio::test]
async fn a_declared_native_client_round_trips_through_the_wire_unchanged() {
    let write = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (201, String::new()))).await;

    provider(&write)
        .create_oidc_client(&realm(), &native_client())
        .await
        .unwrap();

    // The one edit this test performs on the recorded body: Keycloak
    // generates `id`, so `NewClientRepresentation` never carries one while
    // `ClientRepresentation` requires it on read. Any other edit would stop
    // this test proving what it exists to prove — that `declaration` writes
    // what `observe` reads back.
    let written = created_client_body(&write).replacen('{', r#"{"id":"uuid-native-1","#, 1);

    let read = FakeKeycloak::start(Arc::new(move |request: &RecordedRequest| {
        match request.path.as_str() {
            path if path.starts_with("/admin/realms/acme/roles") => (200, "[]".to_owned()),
            path if path.starts_with("/admin/realms/acme/clients") => (200, format!("[{written}]")),
            "/admin/realms/acme" => (200, r#"{"displayName":"Acme"}"#.to_owned()),
            _ => (404, "{}".to_owned()),
        }
    }))
    .await;

    let observed = provider(&read)
        .observe_realm(&realm())
        .await
        .unwrap()
        .expect("the realm exists");

    let client = observed
        .clients
        .get(&native_client().id)
        .expect("the client is reported");

    let declared_uris: BTreeSet<RedirectUri> = native_client().redirect.uris().iter().cloned().collect();

    assert_eq!(client.redirect_uris, declared_uris);
    assert_eq!(client.challenge_method, Some(PkceMethod::S256));
    assert_eq!(client.audience_mapper, Some(AUDIENCE.to_owned()));
    assert_eq!(client.unmodellable_redirect_uris, 0);
}

#[tokio::test]
async fn a_full_page_of_clients_is_refused() {
    let full_page: String = (0..2000)
        .map(|index| {
            format!(
                r#"{{"id":"uuid-{index}","clientId":"client-{index}","redirectUris":[],"publicClient":true}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    let keycloak = FakeKeycloak::start(Arc::new(move |request: &RecordedRequest| {
        match request.path.as_str() {
            path if path.starts_with("/admin/realms/acme/roles") => (200, "[]".to_owned()),
            path if path.starts_with("/admin/realms/acme/clients") => (200, format!("[{full_page}]")),
            "/admin/realms/acme" => (200, r#"{"displayName":"Acme"}"#.to_owned()),
            _ => (404, "{}".to_owned()),
        }
    }))
    .await;

    let error = provider(&keycloak).observe_realm(&realm()).await.unwrap_err();

    assert!(matches!(error, ProviderError::Rejected { .. }), "{error}");
    assert!(error.to_string().contains("2000"), "{error}");
}

#[tokio::test]
async fn an_update_carries_the_mapper_and_attributes_again() {
    let keycloak = FakeKeycloak::start(Arc::new(|request: &RecordedRequest| {
        if request.method == "GET" {
            (
                200,
                r#"[{"id":"uuid-1","clientId":"web","redirectUris":[],"publicClient":true}]"#.to_owned(),
            )
        } else {
            (201, String::new())
        }
    }))
    .await;
    let provider = provider(&keycloak);

    provider
        .create_oidc_client(&realm(), &web_client())
        .await
        .unwrap();
    provider
        .update_oidc_client(&realm(), &web_client())
        .await
        .unwrap();

    let requests = keycloak.admin_requests();
    let post = requests
        .iter()
        .find(|request| request.method == "POST")
        .expect("the create must have happened");
    let put = requests
        .iter()
        .find(|request| request.method == "PUT")
        .expect("the update must have happened");

    assert_eq!(
        post.body, put.body,
        "an update must send the same declaration as a create, or the mapper and attributes \
         written once could silently fall out of step on the next sweep"
    );

    // `post.body == put.body` alone would also pass if both calls dropped the
    // mapper and attributes identically — it proves the two calls agree, not
    // that either carries the right thing. Pin the PUT body's own content so
    // this test is mutation-proof by itself, not only in combination with
    // `a_declared_client_is_written_with_the_platform_audience_mapper` and its
    // siblings.
    assert!(put.body.contains(r#""pkce.code.challenge.method":"S256""#));
    assert!(put.body.contains(r#""post.logout.redirect.uris":"+""#));
    assert!(put.body.contains(r#""oidc-audience-mapper""#));
    assert!(put
        .body
        .contains(&format!(r#""included.custom.audience":"{AUDIENCE}""#)));
}

#[tokio::test]
async fn a_custom_scheme_strategy_is_refused_by_the_adapter_not_written_empty() {
    let keycloak = FakeKeycloak::start(Arc::new(|_: &RecordedRequest| (201, String::new()))).await;

    let error = provider(&keycloak)
        .create_oidc_client(&realm(), &custom_scheme_client())
        .await
        .unwrap_err();

    assert!(matches!(error, ProviderError::Rejected { .. }), "{error}");
    assert!(
        keycloak.admin_requests().is_empty(),
        "a client this adapter cannot write must never reach Keycloak at all"
    );
}
