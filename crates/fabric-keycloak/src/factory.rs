//! Building a Keycloak provider that acts as one operator.

use std::sync::Arc;

use fabric_control_plane::{IdentityProviderFactory, OperatorToken};
use fabric_reconciliation::IdentityProvider;

use crate::{KeycloakConfig, KeycloakIdentityProvider};

/// Builds providers against one Keycloak, per operator.
///
/// Holds configuration and nothing else. There is no credential here, which is
/// the point: the platform has no authority over Keycloak of its own, and
/// every provider this makes is borrowing a person's.
pub struct KeycloakProviderFactory {
    /// Where Keycloak is and how it is addressed.
    config: KeycloakConfig,
}

impl KeycloakProviderFactory {
    /// Builds a factory.
    ///
    /// # Errors
    ///
    /// Returns a message if the configuration is invalid. Checked here, once
    /// at startup, rather than on the first operator's first request — a bad
    /// base URL should not present as an authorisation failure.
    pub fn new(config: &KeycloakConfig) -> Result<Self, String> {
        config.validate()?;

        Ok(Self {
            config: config.clone(),
        })
    }
}

impl IdentityProviderFactory for KeycloakProviderFactory {
    fn acting_as(&self, authority: &OperatorToken) -> Arc<dyn IdentityProvider> {
        // Infallible from here: the configuration was validated at startup and
        // the only remaining failure is building an HTTP client, which does
        // not depend on anything an operator supplied. A provider that refuses
        // every call is a better answer than a `Result` every caller has to
        // thread through for a case that cannot arise.
        match KeycloakIdentityProvider::new(&self.config, authority.expose()) {
            Ok(provider) => Arc::new(provider),
            Err(detail) => Arc::new(crate::unavailable::Unavailable::new(detail)),
        }
    }

    fn describe(&self) -> String {
        format!("keycloak at {}, acting as each operator", self.config.base_url)
    }
}
