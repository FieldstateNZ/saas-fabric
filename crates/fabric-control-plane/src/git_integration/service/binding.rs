//! Pointing the platform at the repository an integration describes.

use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::git_integration::{GitIntegration, SecretName, SecretValue};
use crate::logging;

impl GitIntegrationService {
    /// Binds desired state from whatever is stored, at startup.
    ///
    /// Deliberately returns nothing and fails at nothing. A platform whose
    /// secret store is briefly unreachable must still start: it reports itself
    /// unconfigured, the console still loads, and the next connection attempt
    /// or restart picks the integration back up. Refusing to start would take
    /// away the one tool for diagnosing why.
    pub async fn restore(&self) {
        let integration = match self.store.load(self.kind).await {
            Ok(Some(integration)) => integration,
            Ok(None) => return,
            Err(error) => {
                logging::integration_restore_failed(&error.to_string());
                return;
            }
        };

        let Ok(key) = self.private_key().await else {
            logging::integration_restore_failed("the application's key could not be read");
            return;
        };

        if let Err(error) = self.rebind(&integration, &key) {
            logging::integration_restore_failed(&error);
        }
    }

    /// The application's private key.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::NotConnected`] when there is no key, which
    /// is the state of a platform that has never connected — and
    /// [`IntegrationError::Unavailable`] when the store could not be read.
    pub(super) async fn private_key(&self) -> Result<SecretValue, IntegrationError> {
        self.secrets
            .get(&SecretName::new(self.kind.private_key()))
            .await?
            .ok_or(IntegrationError::NotConnected)
    }

    /// Points the binding at this integration, or leaves it unbound.
    ///
    /// An integration with no repository settled is not an error: it is an
    /// application that is installed and waiting for somebody to say where
    /// client configuration lives. The platform reports itself unconfigured
    /// until it knows.
    pub(super) fn rebind(&self, integration: &GitIntegration, key: &SecretValue) -> Result<(), String> {
        if !integration.is_usable() {
            self.binding.unbind();
            return Ok(());
        }

        let repository = self.factory.build(integration, key)?;
        let described = repository.describe();
        self.binding.bind(repository);
        logging::integration_bound(&described);

        Ok(())
    }
}
