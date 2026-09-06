//! Tests for the rules in [`validation`](super::validation).

use crate::identity::required_roles::REQUIRED_ROLES;
use crate::{ClientProtocol, DesiredStateError, IdentityConfiguration, OidcClient, RedirectUri};
use crate::{OidcClientId, PkceMethod, RealmName, RedirectStrategy, RedirectStrategyKind, RoleName};

fn role(name: &str) -> RoleName {
    RoleName::try_new(name).unwrap()
}

fn valid() -> IdentityConfiguration {
    IdentityConfiguration {
        realm: RealmName::try_new("acme").unwrap(),
        roles: REQUIRED_ROLES.into_iter().map(role).collect(),
        clients: vec![web_client()],
    }
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

#[test]
fn a_complete_configuration_is_accepted() {
    assert_eq!(valid().validate(), Ok(()));
}

#[test]
fn removing_a_required_role_is_refused_and_names_it() {
    let mut identity = valid();
    identity
        .roles
        .retain(|existing| existing.as_str() != "Client Realm User");

    assert_eq!(
        identity.validate(),
        Err(DesiredStateError::RequiredRoleMissing {
            role: "Client Realm User"
        })
    );
}

#[test]
fn a_duplicate_role_is_refused() {
    let mut identity = valid();
    identity.roles.push(role("Client Realm User"));

    assert!(matches!(
        identity.validate(),
        Err(DesiredStateError::Duplicate { field, .. }) if field == "spec.identity.roles"
    ));
}

#[test]
fn two_application_clients_may_not_share_an_id() {
    let mut identity = valid();
    identity.clients.push(web_client());

    assert!(matches!(
        identity.validate(),
        Err(DesiredStateError::Duplicate { field, .. }) if field == "spec.identity.clients"
    ));
}

#[test]
fn declaring_no_application_clients_is_a_legitimate_state() {
    let mut identity = valid();
    identity.clients.clear();

    assert_eq!(identity.validate(), Ok(()));
}

#[test]
fn extra_roles_beyond_the_required_pair_are_allowed() {
    let mut identity = valid();
    identity.roles.push(role("Invoicing Approver"));

    assert_eq!(identity.validate(), Ok(()));
}
