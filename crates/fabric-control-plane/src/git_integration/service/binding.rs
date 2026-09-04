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
            // Not reported as unusable: an unreadable store says nothing
            // about whether a record is in it, and claiming "connected, and
            // failing" about a platform nobody ever connected is the same
            // lie in the other direction.
            Err(error) => {
                logging::integration_restore_failed(&error.to_string());
                return;
            }
        };

        // A record exists, so from here on a failure is a *connected*
        // integration that does not work. Saying "nothing is connected" about
        // one an operator connected last week sends them to connect it again
        // instead of to the reason it stopped.
        let Ok(key) = self.private_key().await else {
            let detail = "the application's key could not be read";
            logging::integration_restore_failed(detail);
            self.target.unusable(detail).await;
            return;
        };

        if let Err(error) = self.rebind(&integration, &key).await {
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

    /// Records the integration, then points the platform at what it describes.
    ///
    /// Both places an operator can settle a repository do exactly this, and the
    /// order is not interchangeable: a binding that worked against a record
    /// nobody stored would be gone at the next restart, and the operator would
    /// be looking at a platform that worked until it quietly didn't.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError`] if the store refused, or
    /// [`IntegrationError::Refused`] if the integration does not describe a
    /// repository this platform can build a client for.
    pub(super) async fn store_and_bind(
        &self,
        integration: &GitIntegration,
        key: &SecretValue,
    ) -> Result<(), IntegrationError> {
        self.store.save(self.kind, integration).await?;
        self.rebind(integration, key)
            .await
            .map_err(IntegrationError::Refused)
    }

    /// Points the binding at this integration, or leaves it unbound.
    ///
    /// An integration with no repository settled is not an error: it is an
    /// application that is installed and waiting for somebody to say where
    /// client configuration lives. The platform reports itself unconfigured
    /// until it knows.
    ///
    /// Awaits the target, which is what lets a binding *drain*: pointing the
    /// platform somewhere new returns only once the operations already running
    /// against where it used to point have finished.
    pub(super) async fn rebind(&self, integration: &GitIntegration, key: &SecretValue) -> Result<(), String> {
        if !integration.is_usable() {
            self.target.unbind().await;
            return Ok(());
        }

        // Recorded as well as returned. The caller is sometimes an operator
        // mid-connection, who sees the error — and sometimes a restart, where
        // nobody is watching and the console is the only thing that will say
        // so. A `match` rather than `inspect_err`, because recording it is
        // itself an await and a closure cannot hold one.
        match self.target.bind(integration, key).await {
            Ok(()) => Ok(()),
            Err(error) => {
                self.target.unusable(&error).await;
                Err(error)
            }
        }
    }
}
