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
    /// going first: every operation already running against the repository has
    /// an outcome before this returns, and none starts against it afterwards.
    /// Cancelling this request cannot shorten that wait, because the operations
    /// run in tasks of their own rather than inside whoever asked for them.
    ///
    /// The wait is bounded by the adapter's operation budget plus the one call
    /// to the Git host the budget cannot cut short, which startup has checked
    /// together fit inside one request.
    ///
    /// # What is still not promised, and cannot be
    ///
    /// A request the platform stopped waiting for is not a request the host
    /// stopped applying. If a call times out on a ref update the host is still
    /// processing, the platform is told the write failed and the host may
    /// commit it a moment after this returns. Nothing here can withdraw it, and
    /// nothing reports it as done — the next read is what sees it.
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
