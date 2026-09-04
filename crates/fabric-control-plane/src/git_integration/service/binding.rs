//! Pointing the platform at the repository an integration describes.

use std::sync::Arc;

use crate::git_integration::service::settling::settling;
use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::git_integration::{GitIntegration, IntegrationTarget, SecretValue};

impl GitIntegrationService {
    /// Records the integration, then points the platform at what it describes.
    ///
    /// Both places an operator can settle a repository do exactly this, and the
    /// order is not interchangeable: a binding that worked against a record
    /// nobody stored would be gone at the next restart, leaving the operator a
    /// platform that worked until it quietly didn't.
    ///
    /// The two are **one transition**, and it runs in a task this only awaits.
    /// A request cut off between the save and the settled binding does not stop
    /// it: the operator may see `504` and the platform converges anyway, so
    /// asking again is safe rather than the only repair. See `settling.rs`.
    ///
    /// `settled` is the caller's record of who did this, and it runs inside the
    /// transition, last. A request that stopped waiting still changed what the
    /// platform points at, and a change that lands must be accounted for by
    /// whoever asked for it — logged from the caller, it would be the one case
    /// where the change lands and the line does not. Last also keeps it off a
    /// refusal: a transition the order turned away never runs at all.
    ///
    /// **This is the transition that checks.** `prepared_against` is the
    /// generation its caller read before the record and the key it writes with —
    /// an installation completing, or an operator choosing a repository, each
    /// holding a pair it obtained while away talking to the Git host. A
    /// disconnect that took its turn in the meantime has made both fiction, and
    /// refusing here is what stops the writes below from putting it back.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError`] if the store refused, or
    /// [`IntegrationError::Refused`] if the integration does not describe a
    /// repository this platform can build a client for — by which point the
    /// record is written and nobody is named for it: `settled` runs only when
    /// the platform points where the operator asked. The target has been told it
    /// is unusable; one with nowhere to say so leaves the record and the binding
    /// disagreeing until the next transition.
    /// [`IntegrationError::Moved`] if the integration changed while this request
    /// was being prepared, in which case nothing is written at all, and
    /// [`IntegrationError::Unavailable`] for a transition nothing watched to the
    /// end, which is not the same as one that failed.
    pub(super) async fn store_and_bind(
        &self,
        prepared_against: u64,
        integration: &GitIntegration,
        key: &SecretValue,
        settled: impl FnOnce() + Send + 'static,
    ) -> Result<(), IntegrationError> {
        // Owned copies, because the task outlives the borrows they arrived as.
        // The collaborators are shared already, so cloning them costs a
        // refcount; the record and the key are small enough to copy outright.
        let kind = self.kind;
        let store = Arc::clone(&self.store);
        let target = Arc::clone(&self.target);
        let integration = integration.clone();
        let key = key.clone();

        let transition = async move {
            store.save(kind, &integration).await?;
            point(&target, &integration, &key)
                .await
                .map_err(IntegrationError::Refused)?;

            settled();

            Ok(())
        };

        settling(Arc::clone(&self.transitions), Some(prepared_against), transition).await
    }
}

/// Points the target at this integration, or leaves it unbound.
///
/// An integration with no repository settled is not an error: it is an
/// application that is installed and waiting for somebody to say where client
/// configuration lives. The platform reports itself unconfigured until it
/// knows.
///
/// Awaits the target, which is what lets a binding *drain*: pointing the
/// platform somewhere new returns only once the operations already running
/// against where it used to point have finished.
///
/// A free function rather than a method, because the task it runs in has no
/// `self` to borrow.
///
/// # Errors
///
/// Returns the target's own message if a client for the repository could not
/// be built.
pub(super) async fn point(
    target: &Arc<dyn IntegrationTarget>,
    integration: &GitIntegration,
    key: &SecretValue,
) -> Result<(), String> {
    if !integration.is_usable() {
        target.unbind().await;
        return Ok(());
    }

    // Recorded as well as returned. The caller is sometimes an operator
    // mid-connection, who sees the error — and sometimes a restart, where
    // nobody is watching and the console is the only thing that will say
    // so. A `match` rather than `inspect_err`, because recording it is
    // itself an await and a closure cannot hold one.
    match target.bind(integration, key).await {
        Ok(()) => Ok(()),
        Err(error) => {
            target.unusable(&error).await;
            Err(error)
        }
    }
}
