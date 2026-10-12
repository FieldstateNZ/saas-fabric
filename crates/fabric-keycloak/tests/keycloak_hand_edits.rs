//! D01-19b over the wire: a hand edit made in Keycloak's own console is held,
//! a product-owned field beside it is still corrected without overwriting
//! it, and only a re-assertion writes the declaration whole.
//!
//! Each test runs the real `IdentityReconciler` over the real adapter against
//! a fake Keycloak on a socket, so what is asserted is the request Keycloak
//! would actually receive — or that it receives none.

// A test's helpers assert their own preconditions; `unwrap` there is the
// assertion, not a hole. See `keycloak_adapter.rs` for the same allowance.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_client_model::{
    AuthorizationConfiguration, Client, ClientId, ClientProtocol, IdentityConfiguration, LifecycleRecord,
    OidcClient, OidcClientId, PkceMethod, RealmName, RedirectStrategy, RedirectStrategyKind, RedirectUri,
};
use fabric_keycloak::KeycloakIdentityProvider;
use fabric_reconciliation::{HandEditedField, HeldEdits, IdentityProvider, IdentityReconciler};
use support::{config_for_tests, FakeKeycloak, RecordedRequest};

const AUDIENCE: &str = "saas-fabric-data-api";

const CALLBACK: &str = "https://www.example.com/callback";

const SILENT: &str = "https://www.example.com/silent";

/// `web`, declaring two callbacks.
fn web_client() -> OidcClient {
    OidcClient {
        id: OidcClientId::try_new("web").unwrap(),
        protocol: ClientProtocol::Oidc,
        pkce: PkceMethod::S256,
        redirect: RedirectStrategy::try_new(
            RedirectStrategyKind::ClaimedHttps,
            vec![
                RedirectUri::try_new(CALLBACK).unwrap(),
                RedirectUri::try_new(SILENT).unwrap(),
            ],
        )
        .unwrap(),
    }
}

/// A client whose realm is `acme`, declaring no roles and `web`.
fn acme() -> Client {
    Client {
        id: ClientId::try_new("acme").unwrap(),
        display_name: "Acme".to_owned(),
        hosts: Vec::new(),
        identity: IdentityConfiguration {
            realm: RealmName::try_new("acme").unwrap(),
            roles: Vec::new(),
            clients: vec![web_client()],
        },
        authorization: AuthorizationConfiguration::default(),
        secrets: None,
        data: BTreeMap::new(),
        lifecycle: LifecycleRecord::default(),
    }
}

/// What Keycloak reports for `web`: converged, then edited by hand as the
/// arguments say.
fn web_as_keycloak_holds_it(enabled: bool, public: bool) -> String {
    format!(
        r#"[{{"id":"uuid-1","clientId":"web","redirectUris":["{CALLBACK}","{SILENT}"],"publicClient":{public},"enabled":{enabled},"standardFlowEnabled":true,"attributes":{{"pkce.code.challenge.method":"S256","post.logout.redirect.uris":"+"}},"protocolMappers":[{{"protocolMapper":"oidc-audience-mapper","config":{{"included.custom.audience":"{AUDIENCE}"}}}}]}}]"#
    )
}

/// A Keycloak holding the `acme` realm, with `web` as `clients` says.
async fn keycloak_holding(clients: String) -> FakeKeycloak {
    FakeKeycloak::start(Arc::new(move |request: &RecordedRequest| {
        match (request.method.as_str(), request.path.as_str()) {
            ("PUT", _) => (204, String::new()),
            (_, path) if path.starts_with("/admin/realms/acme/roles") => (200, "[]".to_owned()),
            (_, path) if path.starts_with("/admin/realms/acme/clients") => (200, clients.clone()),
            (_, "/admin/realms/acme") => (200, r#"{"displayName":"Acme"}"#.to_owned()),
            _ => (404, "{}".to_owned()),
        }
    }))
    .await
}

fn reconciler(keycloak: &FakeKeycloak) -> IdentityReconciler {
    let config = config_for_tests(&keycloak.base_url, AUDIENCE);
    let provider = KeycloakIdentityProvider::new(&config, "an-operators-token").unwrap();

    IdentityReconciler::new(Arc::new(provider))
}

/// The one `PUT` the fake received.
fn put_body(keycloak: &FakeKeycloak) -> String {
    let puts: Vec<_> = keycloak
        .admin_requests()
        .into_iter()
        .filter(|request| request.method == "PUT")
        .collect();
    assert_eq!(puts.len(), 1, "exactly one write was expected");

    puts.into_iter().next().unwrap().body
}

#[tokio::test]
async fn a_client_disabled_in_keycloak_is_held_and_never_written() {
    let keycloak = keycloak_holding(web_as_keycloak_holds_it(false, true)).await;
    let reconciler = reconciler(&keycloak);

    for _ in 0..3 {
        let outcome = reconciler.reconcile(&acme()).await;
        assert!(outcome.changed_nothing());
        assert_eq!(outcome.hand_edits().len(), 1);
        assert_eq!(outcome.hand_edits()[0].field, HandEditedField::ClientDisabled);
    }

    assert_eq!(
        keycloak.count("PUT", "/admin"),
        0,
        "a held hand edit must never be written, however many passes run"
    );
}

#[tokio::test]
async fn a_correction_beside_a_held_disable_writes_the_client_disabled() {
    let keycloak = keycloak_holding(web_as_keycloak_holds_it(false, false)).await;

    let outcome = reconciler(&keycloak).reconcile(&acme()).await;

    assert_eq!(outcome.actions(), 1);
    let body = put_body(&keycloak);
    assert!(
        body.contains(r#""publicClient":true"#),
        "product-owned drift is corrected: {body}"
    );
    assert!(
        body.contains(r#""enabled":false"#),
        "the held disable is preserved: {body}"
    );
}

#[tokio::test]
async fn reasserting_writes_the_declaration_over_the_hand_edit() {
    let keycloak = keycloak_holding(web_as_keycloak_holds_it(false, true)).await;

    let outcome = reconciler(&keycloak).reassert(&acme()).await;

    assert_eq!(outcome.overwritten().len(), 1);
    let body = put_body(&keycloak);
    assert!(body.contains(r#""enabled":true"#), "{body}");
    assert!(body.contains(r#""standardFlowEnabled":true"#), "{body}");
}

#[tokio::test]
async fn an_update_leaves_a_held_removed_callback_out() {
    let keycloak = keycloak_holding(web_as_keycloak_holds_it(true, true)).await;
    let held = HeldEdits {
        removed_redirect_uris: [RedirectUri::try_new(SILENT).unwrap()].into_iter().collect(),
        ..HeldEdits::default()
    };

    let config = config_for_tests(&keycloak.base_url, AUDIENCE);
    KeycloakIdentityProvider::new(&config, "an-operators-token")
        .unwrap()
        .update_oidc_client(&acme().identity.realm, &web_client(), &held)
        .await
        .unwrap();

    let body = put_body(&keycloak);
    assert!(
        body.contains(&format!(r#""redirectUris":["{CALLBACK}"]"#)),
        "{body}"
    );
}
