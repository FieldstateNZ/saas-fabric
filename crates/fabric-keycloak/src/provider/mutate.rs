//! Making Keycloak match what was planned.

use fabric_client_model::{OidcClient, OidcClientId, RealmName, RoleName};
use fabric_reconciliation::ProviderError;

use crate::admin::KeycloakAdmin;
use crate::wire::{
    ClientRepresentation, NewClientRepresentation, NewRealmRepresentation, NewRoleRepresentation, RealmUpdate,
};

/// Creates a realm, or does nothing if it already exists.
pub(super) async fn create_realm(
    admin: &KeycloakAdmin,
    realm: &RealmName,
    display_name: &str,
) -> Result<(), ProviderError> {
    let body = NewRealmRepresentation {
        realm: realm.as_str(),
        display_name,
        enabled: true,
    };

    admin
        .create("creating the realm", admin.paths().realms(), &body)
        .await
}

/// Sets a realm's display name, leaving every other setting alone.
pub(super) async fn set_realm_display_name(
    admin: &KeycloakAdmin,
    realm: &RealmName,
    display_name: &str,
) -> Result<(), ProviderError> {
    let body = RealmUpdate {
        realm: realm.as_str(),
        display_name,
    };

    admin
        .update("updating the realm", admin.paths().realm(realm), &body)
        .await
}

/// Creates a realm role, or does nothing if it already exists.
pub(super) async fn create_realm_role(
    admin: &KeycloakAdmin,
    realm: &RealmName,
    role: &RoleName,
) -> Result<(), ProviderError> {
    let body = NewRoleRepresentation { name: role.as_str() };

    admin
        .create("creating a realm role", admin.paths().roles(realm), &body)
        .await
}

/// Creates an application client, or does nothing if it already exists.
pub(super) async fn create_oidc_client(
    admin: &KeycloakAdmin,
    realm: &RealmName,
    client: &OidcClient,
) -> Result<(), ProviderError> {
    admin
        .create(
            "creating an application client",
            admin.paths().clients(realm),
            &declaration(client),
        )
        .await
}

/// Brings an existing application client in line with its declaration.
///
/// Two calls, because Keycloak addresses an update by its own internal
/// identifier rather than by the `clientId` the platform knows — see
/// [`ClientRepresentation`]. The lookup is not wasted work: it is also what
/// distinguishes "the client is gone" from "the update failed", and a client
/// that has disappeared between the observation and the update is created on
/// the next pass rather than reported as a mystery.
pub(super) async fn update_oidc_client(
    admin: &KeycloakAdmin,
    realm: &RealmName,
    client: &OidcClient,
) -> Result<(), ProviderError> {
    let Some(internal_id) = internal_id(admin, realm, &client.id).await? else {
        return create_oidc_client(admin, realm, client).await;
    };

    admin
        .update(
            "updating an application client",
            admin.paths().client(realm, &internal_id),
            &declaration(client),
        )
        .await
}

/// Looks up Keycloak's internal identifier for an application client.
async fn internal_id(
    admin: &KeycloakAdmin,
    realm: &RealmName,
    client: &OidcClientId,
) -> Result<Option<String>, ProviderError> {
    let matches: Vec<ClientRepresentation> = admin
        .get(
            "looking up an application client",
            admin.paths().client_by_client_id(realm, client),
        )
        .await?;

    Ok(matches.into_iter().next().map(|found| found.id))
}

/// The representation SaaS Fabric writes for a declared client.
///
/// The same body for create and update, which is what makes an update
/// idempotent: writing the declaration twice produces the same object.
fn declaration(client: &OidcClient) -> NewClientRepresentation<'_> {
    NewClientRepresentation {
        client_id: client.id.as_str(),
        enabled: true,
        protocol: "openid-connect",
        public_client: true,
        standard_flow_enabled: true,
        redirect_uris: client
            .redirect
            .uris()
            .iter()
            .map(|uri| uri.as_str().to_owned())
            .collect(),
    }
}
