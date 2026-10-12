//! The one observe-plan-apply path every entry point shares.

use fabric_client_model::Client;

use crate::ledger::AppliedBaseline;
use crate::logging;
use crate::plan::HandEditPolicy;
use crate::provider::ProviderError;
use crate::reconciler::{apply, IdentityReconciler, ReconciliationOutcome};

impl IdentityReconciler {
    /// Observes, plans under `policy`, applies, and says what happened.
    pub(super) async fn run(&self, client: &Client, policy: HandEditPolicy) -> ReconciliationOutcome {
        let baseline = self.baseline(client).await;
        let plan = match self.plan_under(client, policy, &baseline).await {
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

        let recording = apply::Recording {
            ledger: self.ledger.as_deref(),
            client,
            baseline: &baseline,
        };
        match apply::apply(self.provider.as_ref(), &plan, &recording).await {
            Ok(()) => ReconciliationOutcome::applied(plan.actions().len())
                .holding(held)
                .overwriting(overwritten),
            Err(apply::Interrupted {
                completed,
                error,
                recorded_late,
            }) => {
                logging::apply_failed(client, &error);
                let (confirmed, unconfirmed) = apply::overwrites_after(&plan, completed, &overwritten);
                let unconfirmed = if recorded_late { Vec::new() } else { unconfirmed };
                ReconciliationOutcome::failed_after(completed, &error)
                    .overwriting(confirmed)
                    .unconfirmed(unconfirmed)
            }
        }
    }
}

impl IdentityReconciler {
    /// What the ledger holds for `client`, or nothing when there is no
    /// ledger or it cannot be read — which holds, never writes, whatever is
    /// ambiguous.
    pub(super) async fn baseline(&self, client: &Client) -> AppliedBaseline {
        let Some(ledger) = &self.ledger else {
            return AppliedBaseline::unknown();
        };

        ledger.read(client).await.unwrap_or_else(|error| {
            logging::baseline_unreadable(client, &error);
            AppliedBaseline::unknown()
        })
    }
}
