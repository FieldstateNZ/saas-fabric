//! A provider that reports why it could not be built.
//!
//! Exists so that [building one](crate::KeycloakProviderFactory) can be
//! infallible. The alternative is a `Result` on every call site for a failure
//! that depends on nothing the caller did — and a `Result` nobody can act on
//! tends to become an `unwrap` eventually.

use async_trait::async_trait;
use fabric_client_model::{OidcClient, RealmName, RoleName};
use fabric_reconciliation::{IdentityProvider, ObservedRealm, ProviderError};

/// Refuses every operation, with the reason it exists.
pub(crate) struct Unavailable {
    /// Why the real provider could not be built.
    detail: String,
}

impl Unavailable {
    /// Wraps a construction failure.
    pub(crate) fn new(detail: String) -> Self {
        Self { detail }
    }

    /// The failure, as a provider error.
    fn refusal(&self) -> ProviderError {
        ProviderError::Unavailable {
            detail: self.detail.clone(),
        }
    }
}

#[async_trait]
impl IdentityProvider for Unavailable {
    async fn observe_realm(&self, _realm: &RealmName) -> Result<Option<ObservedRealm>, ProviderError> {
        Err(self.refusal())
    }

    async fn create_realm(&self, _realm: &RealmName, _display_name: &str) -> Result<(), ProviderError> {
        Err(self.refusal())
    }

    async fn set_realm_display_name(
        &self,
        _realm: &RealmName,
        _display_name: &str,
    ) -> Result<(), ProviderError> {
        Err(self.refusal())
    }

    async fn create_realm_role(&self, _realm: &RealmName, _role: &RoleName) -> Result<(), ProviderError> {
        Err(self.refusal())
    }

    async fn create_oidc_client(
        &self,
        _realm: &RealmName,
        _client: &OidcClient,
    ) -> Result<(), ProviderError> {
        Err(self.refusal())
    }

    async fn update_oidc_client(
        &self,
        _realm: &RealmName,
        _client: &OidcClient,
    ) -> Result<(), ProviderError> {
        Err(self.refusal())
    }

    fn configured_audience(&self) -> Option<&str> {
        // Never actually read: `IdentityReconciler::plan` only asks for this
        // after `observe_realm` succeeds, and every method here refuses.
        // `None` is still the honest answer if that ever changes: this type
        // exists because the real provider could not be built, so it has no
        // audience to report either.
        None
    }

    fn describe(&self) -> String {
        "an identity provider that could not be built".to_owned()
    }
}
