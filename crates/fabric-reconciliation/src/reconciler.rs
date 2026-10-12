//! Deciding what to change, doing it, and saying what happened.
//!
//! In the 121–150 line band: one type whose three entry points — plan,
//! reconcile, re-assert — share one observe-plan-apply path, which is the
//! point. A re-assertion that ran a different path from an ordinary pass
//! could diverge from it in what it observes or how it reports.

mod apply;
mod outcome;
#[cfg(test)]
mod reconciler_tests;

use std::sync::Arc;

use fabric_client_model::Client;

use crate::logging;
use crate::plan::{self, HandEditPolicy, IdentityPlan};
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
    /// Unsupported hand edits are held, as an ordinary pass holds them.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] if the provider's current state could not be
    /// read, or if the provider names no configured audience at all
    /// ([`ProviderError::NoAudienceConfigured`]). Either way there is nothing
    /// safe to compare against, so there is no plan — an empty one would be
    /// indistinguishable from "already converged", which is the most
    /// dangerous possible answer to give here.
    pub async fn plan(&self, client: &Client) -> Result<IdentityPlan, ProviderError> {
        self.plan_under(client, HandEditPolicy::Hold).await
    }

    /// Brings the provider in line with the desired state, holding every
    /// unsupported hand edit rather than fighting it (D01-19b).
    ///
    /// Idempotent: a second call over an unchanged desired state produces an
    /// empty plan and makes no changing calls at all — including when a hand
    /// edit is held, which is what stops a pass that overwrote it and a
    /// person who re-made it from taking turns forever.
    pub async fn reconcile(&self, client: &Client) -> ReconciliationOutcome {
        self.run(client, HandEditPolicy::Hold).await
    }

    /// Re-asserts the projection: brings the provider in line with the
    /// desired state, overwriting every unsupported hand edit.
    ///
    /// Only an operator's explicit request reaches this, and the caller
    /// audits it with [`ReconciliationOutcome::overwritten`]. It acts with
    /// the same provider, and so the same authority, as an ordinary pass.
    pub async fn reassert(&self, client: &Client) -> ReconciliationOutcome {
        self.run(client, HandEditPolicy::Overwrite).await
    }

    /// Observes and plans under `policy`.
    async fn plan_under(
        &self,
        client: &Client,
        policy: HandEditPolicy,
    ) -> Result<IdentityPlan, ProviderError> {
        let observed = self.provider.observe_realm(&client.identity.realm).await?;

        let audience = self
            .provider
            .configured_audience()
            .ok_or(ProviderError::NoAudienceConfigured)?;

        Ok(plan::plan(client, observed.as_ref(), audience, policy))
    }

    /// Observes, plans under `policy`, applies, and says what happened.
    async fn run(&self, client: &Client, policy: HandEditPolicy) -> ReconciliationOutcome {
        let plan = match self.plan_under(client, policy).await {
            Ok(plan) => plan,
            Err(error @ ProviderError::NoAudienceConfigured) => {
                logging::planning_refused(client, &error);
                return ReconciliationOutcome::failed(&error);
            }
            Err(error) => {
                logging::observation_failed(client, &error);
                return ReconciliationOutcome::failed(&error);
            }
        };

        let found = plan.hand_edits().to_vec();
        let (held, overwritten) = match policy {
            HandEditPolicy::Hold => (found, Vec::new()),
            HandEditPolicy::Overwrite => (Vec::new(), found),
        };

        if !held.is_empty() {
            logging::hand_edits_held(client, &held);
        }

        if plan.is_converged() {
            logging::already_converged(client);
            return ReconciliationOutcome::converged().holding(held);
        }

        logging::applying(client, &plan, &overwritten);

        match apply::apply(self.provider.as_ref(), &plan).await {
            Ok(()) => ReconciliationOutcome::applied(plan.actions().len())
                .holding(held)
                .overwriting(overwritten),
            Err(apply::Interrupted { completed, error }) => {
                logging::apply_failed(client, &error);
                let (confirmed, unconfirmed) = apply::overwrites_after(&plan, completed, &overwritten);
                ReconciliationOutcome::failed_after(completed, &error)
                    .overwriting(confirmed)
                    .unconfirmed(unconfirmed)
            }
        }
    }
}
