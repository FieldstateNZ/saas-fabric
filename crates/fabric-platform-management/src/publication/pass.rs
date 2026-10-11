//! What one publication pass does: read the platform's inputs, compose a
//! snapshot, and offer it -- the body [`crate::RuntimePublisher::publish_once`]
//! runs once its re-entry guard has let it through.

#[cfg(test)]
#[path = "pass_tests.rs"]
mod pass_tests;

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, TenantId};
use fabric_runtime_publication::PublishedRevisions;

use crate::publication::outcome::{PassOutcome, WaitingReason};
use crate::publication::protocol::publish_with_retry;
use crate::publication::publish_error::outcome_from_publish_error;
use crate::publication::report_outcome::outcome_from_report;
use crate::publication::snapshot::compose;
use crate::publication::withhold::{publishable, published_tenants};
use crate::publication::RuntimePublisher;
use crate::SafeDiagnostic;

/// Where reading an input lands, before there is a [`PassOutcome`] to
/// build: a coherence problem this platform itself wrote, or a catalogue
/// conflict, is [`Self::Refused`]; a transport failure reaching any input
/// is [`Self::Failed`]; no platform repository connected yet is
/// [`Self::Waiting`], the one halt that is not a problem at all. Built by
/// `publication::reads`, consumed here.
pub(super) enum Halt {
    Refused(SafeDiagnostic),
    Failed(SafeDiagnostic),
    Waiting(WaitingReason),
}

impl Halt {
    fn into_outcome(self) -> PassOutcome {
        match self {
            Self::Refused(reason) => PassOutcome::Refused { reason },
            Self::Failed(detail) => PassOutcome::Failed { detail },
            Self::Waiting(reason) => PassOutcome::Waiting { reason },
        }
    }
}

/// What a pass did, and the tenants it left published when it published or
/// found nothing to change.
type PassRun = (PassOutcome, Option<BTreeMap<TenantId, BindingRevision>>);

impl RuntimePublisher {
    /// Runs one pass: read, compose, offer -- without the re-entry guard
    /// [`crate::RuntimePublisher::publish_once`] holds around this call.
    pub(super) async fn run_pass(&self) -> PassRun {
        match self.read_and_offer().await {
            Ok(run) => run,
            Err(halt) => (halt.into_outcome(), None),
        }
    }

    async fn read_and_offer(&self) -> Result<PassRun, Halt> {
        let held = match self.target.current().await {
            Ok(held) => held,
            // The same exhaustive classifier `publish_with_retry`'s own
            // failure reaches below, not a blanket `Failed`: `current`'s own
            // contract only ever returns `Unreadable`, which lands there
            // too, but one classifier for every error this port can raise
            // is what keeps that true by construction rather than by both
            // call sites happening to agree.
            Err(error) => return Ok((outcome_from_publish_error(&error), None)),
        };

        let declarations = self.declared_data_sources().await?;
        let placements = self.recorded_placements(&declarations).await?;
        let withheld = self.withheld_tenants().await?;
        let catalog = self.runtime_catalogue().await?;

        if catalog.is_empty() {
            return Err(Halt::Waiting(WaitingReason::NoResources));
        }

        let placements = publishable(placements, &withheld);
        let snapshot = compose(declarations, &placements, catalog, &held)
            .map_err(|error| Halt::Refused(SafeDiagnostic::sanitise(&error.to_string())))?;

        Ok(match publish_with_retry(self.target.as_ref(), snapshot).await {
            Ok((report, snapshot)) => {
                let outcome = outcome_from_report(
                    report,
                    &held,
                    PublishedRevisions {
                        tenants: Some(snapshot.tenants.revision),
                        data_sources: Some(snapshot.data_sources.revision),
                        catalog: Some(snapshot.catalog.revision),
                    },
                );
                (outcome, Some(published_tenants(&snapshot.tenants.payload)))
            }
            Err(error) => (outcome_from_publish_error(&error), None),
        })
    }
}
