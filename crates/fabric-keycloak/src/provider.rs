//! The Keycloak implementation of the identity-provider port.

mod mutate;
mod observe;

use async_trait::async_trait;
use fabric_client_model::{OidcClient, RealmName, RoleName};
use fabric_reconciliation::{IdentityProvider, ObservedRealm, ProviderError};

use crate::admin::KeycloakAdmin;
use crate::KeycloakConfig;

/// Reconciles client identity into Keycloak.
///
/// # It performs; it does not decide
///
/// Every method here maps one platform operation onto one or two admin API
/// calls. There is no diffing, no ordering logic, and no notion of drift —
/// those belong to `fabric-reconciliation`, which is what keeps this crate
/// replaceable and that one testable without a Keycloak.
pub struct KeycloakIdentityProvider {
    /// The admin API client.
    admin: KeycloakAdmin,
}

impl KeycloakIdentityProvider {
    /// Builds a provider that acts with one operator's authority.
    ///
    /// # Errors
    ///
    /// Returns a message if the configuration is invalid or the HTTP client
    /// cannot be built.
    pub fn new(config: &KeycloakConfig, authority: &str) -> Result<Self, String> {
        Ok(Self {
            admin: KeycloakAdmin::new(config, authority)?,
        })
    }
}

#[async_trait]
impl IdentityProvider for KeycloakIdentityProvider {
    async fn observe_realm(&self, realm: &RealmName) -> Result<Option<ObservedRealm>, ProviderError> {
        observe::realm(&self.admin, realm).await
    }

    async fn create_realm(&self, realm: &RealmName, display_name: &str) -> Result<(), ProviderError> {
        mutate::create_realm(&self.admin, realm, display_name).await
    }

    async fn set_realm_display_name(
        &self,
        realm: &RealmName,
        display_name: &str,
    ) -> Result<(), ProviderError> {
        mutate::set_realm_display_name(&self.admin, realm, display_name).await
    }

    async fn create_realm_role(&self, realm: &RealmName, role: &RoleName) -> Result<(), ProviderError> {
        mutate::create_realm_role(&self.admin, realm, role).await
    }

    async fn create_oidc_client(&self, realm: &RealmName, client: &OidcClient) -> Result<(), ProviderError> {
        mutate::create_oidc_client(&self.admin, realm, client).await
    }

    async fn update_oidc_client(&self, realm: &RealmName, client: &OidcClient) -> Result<(), ProviderError> {
        mutate::update_oidc_client(&self.admin, realm, client).await
    }

    fn describe(&self) -> String {
        self.admin.paths().describe()
    }
}
