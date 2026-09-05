//! Picking up whatever an operator connected before the last restart.

use std::sync::Arc;

use crate::git_integration::service::binding::point;
use crate::git_integration::service::settling::settling;
use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::git_integration::{GitIntegration, SecretValue};
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

        // Awaited, so startup still knows the binding settled before anything
        // sweeps through it — the transition is detached from a *caller*, not
        // from whoever waits for it.
        if let Err(error) = self.rebind(&integration, &key).await {
            logging::integration_restore_failed(&error.to_string());
        }
    }

    /// Points the binding at a stored integration, recording nothing.
    ///
    /// A transition of one half rather than two, and it takes its turn with
    /// the rest anyway: this is the third place the live binding is ever
    /// settled, and one outside the order is one that could land after a
    /// transition asked for later.
    ///
    /// It passes no generation, so it cannot be refused for one that moved.
    /// Startup runs before anything else has a turn to take, so there is no
    /// generation for this to be stale against — and a restore that refused
    /// itself would leave a perfectly good stored integration unbound with
    /// nothing to try again.
    ///
    /// # Errors
    ///
    /// [`IntegrationError::Refused`] if the stored integration cannot be made
    /// to work, or [`IntegrationError::Unavailable`] if the transition was not
    /// observed to finish.
    async fn rebind(&self, integration: &GitIntegration, key: &SecretValue) -> Result<(), IntegrationError> {
        let target = Arc::clone(&self.target);
        let integration = integration.clone();
        let key = key.clone();

        settling(Arc::clone(&self.transitions), None, async move {
            point(&target, &integration, &key)
                .await
                .map_err(IntegrationError::Refused)
        })
        .await
    }
}
