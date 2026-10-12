//! Deciding what to change, doing it, and saying what happened.
//!
//! Its three entry points — plan, reconcile, re-assert — share one
//! observe-plan-apply path (`run.rs`), which is the point. A re-assertion
//! that ran a different path from an ordinary pass could diverge from it in
//! what it observes or how it reports.
//!
//! In the 121–150 line band: one type, its constructors and its three entry
//! points, each of whose rustdoc states a contract — the failure shape, the
//! hand-edit policy, the withheld and ledger semantics — that belongs beside
//! the signature it constrains.

mod apply;
#[cfg(test)]
mod ledger_tests;
mod outcome;
#[cfg(test)]
mod reconciler_tests;
mod recorded;
mod run;
mod withhold;
#[cfg(test)]
mod withhold_tests;

use std::sync::Arc;

use fabric_client_model::{Client, OidcClientId};

use crate::ledger::{AppliedBaseline, DeclarationLedger};
use crate::logging;
use crate::plan::{self, HandEditPolicy, IdentityPlan};
use crate::provider::{IdentityProvider, ProviderError};

pub(crate) use apply::ApplyError;
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

    /// Where what Fabric writes is recorded, and read back as the baseline
    /// a missing callback is classified against. `None`: nothing is known,
    /// and every ambiguous transition is held.
    ledger: Option<Arc<dyn DeclarationLedger>>,
}

impl IdentityReconciler {
    /// Builds a reconciler over a provider.
    #[must_use]
    pub fn new(provider: Arc<dyn IdentityProvider>) -> Self {
        Self {
            provider,
            ledger: None,
        }
    }

    /// Records every client write in `ledger` and classifies against what it
    /// holds. See the `ledger` module for what may implement one.
    #[must_use]
    pub fn with_ledger(mut self, ledger: Arc<dyn DeclarationLedger>) -> Self {
        self.ledger = Some(ledger);
        self
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
        let baseline = self.baseline(client).await;
        self.plan_under(client, HandEditPolicy::Hold, &baseline).await
    }

    /// Brings the provider in line with the desired state, holding every
    /// unsupported hand edit rather than fighting it (D01-19b).
    ///
    /// Idempotent: a second call over an unchanged desired state produces an
    /// empty plan and makes no changing calls at all — including when a hand
    /// edit is held, which is what stops a pass that overwrote it and a
    /// person who re-made it from taking turns forever.
    ///
    /// `withheld` names declared application clients the caller does not
    /// trust to converge (D01-19b: a stored projected entry that is not its
    /// projection). They are neither written nor corrected — converged onto
    /// neither the declaration nor anything else — and the outcome names
    /// those actually declared. Everything else converges as usual.
    pub async fn reconcile(&self, client: &Client, withheld: &[OidcClientId]) -> ReconciliationOutcome {
        let (declared, withheld) = withhold::split(client, withheld);
        if !withheld.is_empty() {
            logging::clients_withheld(client, &withheld);
        }

        self.run(&declared, HandEditPolicy::Hold)
            .await
            .withholding(withheld)
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
        baseline: &AppliedBaseline,
    ) -> Result<IdentityPlan, ProviderError> {
        let observed = self.provider.observe_realm(&client.identity.realm).await?;

        let audience = self
            .provider
            .configured_audience()
            .ok_or(ProviderError::NoAudienceConfigured)?;

        Ok(plan::plan(client, observed.as_ref(), audience, policy, baseline))
    }
}
