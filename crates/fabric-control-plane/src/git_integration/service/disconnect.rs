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
    /// start against a repository this platform is being told to forget; then
    /// the key, because a record without a key is harmless while a key without
    /// a record is a credential nothing accounts for.
    ///
    /// Releasing the binding *waits*, and that is the load-bearing part of
    /// going first: a sweep already running against the repository finishes
    /// before this returns, so nothing lands there afterwards. The wait is
    /// bounded by the adapter's operation budget, which startup has checked is
    /// shorter than one request.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError`] if either store refused. The binding is
    /// released by then — a disconnect that got that far and then failed leaves
    /// the platform not using the integration, which is the safe half to land
    /// on.
    ///
    /// # A disconnect that was cut off has done nothing
    ///
    /// This is not an error it can return, because it never gets to return.
    /// If the request timeout or the operator's browser cancels the request,
    /// the handler future is dropped — possibly while still waiting for the
    /// binding — and then nothing has been released, no key deleted and no
    /// record cleared. The operator sees `504` and the integration is exactly
    /// as it was. There is no half-state to repair; the answer is to ask again.
    pub async fn disconnect(&self, operator: &Operator) -> Result<(), IntegrationError> {
        self.target.unbind().await;

        self.secrets
            .delete(&SecretName::new(self.kind.private_key()))
            .await?;
        self.store.clear(self.kind).await?;

        logging::integration_disconnected(operator.subject());

        Ok(())
    }
}
