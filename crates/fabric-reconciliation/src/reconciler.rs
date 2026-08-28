//! Deciding what to change, doing it, and saying what happened.

mod apply;
mod outcome;
#[cfg(test)]
mod reconciler_tests;

use std::sync::Arc;

use fabric_client_model::Client;

use crate::logging;
use crate::plan::{self, IdentityPlan};
use crate::provider::{IdentityProvider, ProviderError};

pub use outcome::ReconciliationOutcome;

/// Converges an identity provider onto a client's desired identity state.
///
/// # Why `reconcile` returns an outcome rather than a `Result`
///
/// A failed reconciliation is not an error the caller is expected to handle —
/// it is a **fact about a client** that has to be recorded and shown, and the
/// next scheduled pass will try again regardless. Returning `Result` would
/// invite a caller to `?` it away, which is exactly the shape that produces a
/// control plane where a client silently never converges and nothing says so.
///
/// [`plan`](Self::plan) does return a `Result`, because a caller asking only
/// what *would* change has nothing to record and genuinely wants the failure.
pub struct IdentityReconciler {
    /// The provider being converged.
    provider: Arc<dyn IdentityProvider>,
}

impl IdentityReconciler {
    /// Builds a reconciler over a provider.
    #[must_use]
    pub fn new(provider: Arc<dyn IdentityProvider>) -> Self {
        Self { provider }
    }

    /// Works out what would have to change, without changing anything.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] if the provider's current state could not be
    /// read. There is nothing to compare against, so there is no plan — an
    /// empty one would be indistinguishable from "already converged", which is
    /// the most dangerous possible answer to give here.
    pub async fn plan(&self, client: &Client) -> Result<IdentityPlan, ProviderError> {
        let observed = self.provider.observe_realm(&client.identity.realm).await?;

        Ok(plan::plan(client, observed.as_ref()))
    }

    /// Brings the provider in line with the desired state.
    ///
    /// Idempotent: a second call over an unchanged desired state produces an
    /// empty plan and makes no changing calls at all.
    pub async fn reconcile(&self, client: &Client) -> ReconciliationOutcome {
        let plan = match self.plan(client).await {
            Ok(plan) => plan,
            Err(error) => {
                logging::observation_failed(client, &error);
                return ReconciliationOutcome::failed(&error);
            }
        };

        if plan.is_converged() {
            logging::already_converged(client);
            return ReconciliationOutcome::converged();
        }

        logging::applying(client, &plan);

        match apply::apply(self.provider.as_ref(), &plan).await {
            Ok(()) => ReconciliationOutcome::applied(plan.actions().len()),
            Err(error) => {
                logging::apply_failed(client, &error);
                ReconciliationOutcome::failed(&error)
            }
        }
    }
}
