//! Desired-state fixtures shared by this crate's tests.
//!
//! One builder rather than one per test module: the diff, the reconciler and
//! the status transition all need the same shape of client, and three
//! near-identical copies would drift until a test was quietly asserting
//! against a client nobody else uses.

use fabric_client_model::{
    Client, ClientId, ClientProtocol, ClientRevision, IdentityConfiguration, OidcClient, OidcClientId,
    RealmName, RedirectUri, RoleName,
};

/// The role names every fixture client declares.
pub(crate) const ROLES: [&str; 2] = ["Client Realm Administrator", "Client Realm User"];

/// Parses a role name that the fixtures know is valid.
pub(crate) fn role(name: &str) -> RoleName {
    RoleName::try_new(name).unwrap()
}

/// A revision, for the tests that need one to compare.
pub(crate) fn revision(value: &str) -> ClientRevision {
    ClientRevision::try_new(value).unwrap()
}

/// The `web` application client every fixture declares.
pub(crate) fn web_client() -> OidcClient {
    OidcClient {
        id: OidcClientId::try_new("web").unwrap(),
        protocol: ClientProtocol::Oidc,
        redirect_uris: vec![
            RedirectUri::try_new("https://www.example.com/callback").unwrap(),
            RedirectUri::try_new("https://www.example.com/silent").unwrap(),
        ],
    }
}

/// A complete, valid client.
pub(crate) fn acme() -> Client {
    Client {
        id: ClientId::try_new("acme").unwrap(),
        display_name: "Acme".to_owned(),
        hosts: Vec::new(),
        identity: IdentityConfiguration {
            realm: RealmName::try_new("acme").unwrap(),
            roles: ROLES.into_iter().map(role).collect(),
            clients: vec![web_client()],
        },
    }
}
