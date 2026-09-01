//! Forgetting an integration.

use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::git_integration::SecretName;
use crate::logging;
use crate::Operator;

impl GitIntegrationService {
    /// Forgets this platform's integration entirely.
    ///
    /// # What this does and does not do
    ///
    /// It removes what *this platform* holds: the record, the private key, and
    /// the binding. It does **not** uninstall the application or delete it on
    /// the host — that is the operator's to do there, and an API that quietly
    /// deleted an organisation's application because somebody clicked
    /// disconnect would be doing considerably more than it said.
    ///
    /// The order is deliberate. The binding goes first, so that no sweep can
    /// use a credential that is about to disappear; then the key, because a
    /// record without a key is harmless while a key without a record is a
    /// credential nothing accounts for.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError`] if either store refused. The binding is
    /// already released by then — a disconnect that half-failed leaves the
    /// platform not using the integration, which is the safe half to land on.
    pub async fn disconnect(&self, operator: &Operator) -> Result<(), IntegrationError> {
        self.binding.unbind();

        self.secrets
            .delete(&SecretName::new(self.kind.private_key()))
            .await?;
        self.store.clear(self.kind).await?;

        logging::integration_disconnected(operator.subject());

        Ok(())
    }
}
