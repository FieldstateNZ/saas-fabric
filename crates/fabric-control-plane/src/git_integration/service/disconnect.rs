//! Forgetting an integration.

use std::sync::Arc;

use crate::git_integration::service::settling::settling;
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
    /// an outcome before the deletions begin, and none starts against it
    /// afterwards. Cancelling this request cannot shorten that wait, because
    /// the operations run in tasks of their own rather than inside whoever
    /// asked for them.
    ///
    /// That wait is bounded by the adapter's operation budget plus the one call
    /// to the Git host the budget cannot cut short. Startup checks that sum is
    /// below the API request timeout, leaving explicit headroom for the rest of
    /// this — the drain is the longest step, not the only one.
    ///
    /// # A request that is cut off does not stop the disconnect
    ///
    /// All three steps run in one transition, in a task of its own, which this
    /// only awaits. If the request timeout fires or the operator's browser goes
    /// away, the handler's future is dropped and the disconnect carries on
    /// regardless: the binding is released, the key deleted and the record
    /// cleared. The operator may see `504` and find the integration gone.
    ///
    /// Asking again is safe — a second disconnect releases a binding already
    /// released and deletes what is already deleted — and it is also *ordered*
    /// against every other transition on this integration, so a disconnect and
    /// a rebind that overlap cannot leave the record and the binding
    /// disagreeing. See `settling.rs`.
    ///
    /// # A disconnect always wins, and nothing prepared earlier can undo it
    ///
    /// A rebind that read the record and the key *before* this took its turn
    /// still holds both when it runs after. It cannot use them: the order it
    /// queues in carries a generation, this disconnect moved it, and a
    /// transition prepared against the generation it left is refused with
    /// [`IntegrationError::Moved`] before a single write. The operator who
    /// asked for the rebind is told to look again and ask again, and what they
    /// see is a platform with nothing connected. See `order.rs`.
    ///
    /// This transition passes no generation of its own, and that is the other
    /// half of the same rule. "Forget it all" is what the operator wants
    /// whatever landed in between, so a disconnect is never refused for state
    /// that moved — it is the thing that moves it. The cost is the mirror of
    /// the creation's: a disconnect queued behind a drain while a creation
    /// lands first deletes that new key and record, and the code that minted
    /// the key is spent, so the remedy is to create again.
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
    /// on. [`IntegrationError::Unavailable`] also stands for a transition
    /// nothing watched to the end, which is not the same as one that failed.
    pub async fn disconnect(&self, operator: &Operator) -> Result<(), IntegrationError> {
        // Owned copies, because the task outlives the borrows they arrived as.
        let kind = self.kind;
        let secrets = Arc::clone(&self.secrets);
        let store = Arc::clone(&self.store);
        let target = Arc::clone(&self.target);
        let subject = operator.subject().to_owned();

        settling(Arc::clone(&self.transitions), None, async move {
            target.unbind().await;

            secrets.delete(&SecretName::new(kind.private_key())).await?;
            store.clear(kind).await?;

            logging::integration_disconnected(&subject);

            Ok(())
        })
        .await
    }
}
