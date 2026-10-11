//! What one publication pass does: read the platform's inputs, compose a
//! snapshot, and offer it -- the body [`crate::RuntimePublisher::publish_once`]
//! runs once its re-entry guard has let it through.

#[cfg(test)]
#[path = "pass_tests.rs"]
mod pass_tests;

use fabric_runtime_publication::PublishedRevisions;

use crate::publication::catalogue_hold::hold_unbound_catalogue;
use crate::publication::outcome::{PassOutcome, WaitingReason};
use crate::publication::publish_error::outcome_from_publish_error;
use crate::publication::report_outcome::outcome_from_report;
use crate::publication::snapshot::compose;
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

impl RuntimePublisher {
    /// Runs one pass: read, compose, offer -- without the re-entry guard
    /// [`crate::RuntimePublisher::publish_once`] holds around this call.
    pub(super) async fn run_pass(&self) -> PassOutcome {
        let held = match self.target.current().await {
            Ok(held) => held,
            // The same exhaustive classifier `publish_with_retry`'s own
            // failure reaches below, not a blanket `Failed`: `current`'s own
            // contract only ever returns `Unreadable`, which lands there
            // too, but one classifier for every error this port can raise
            // is what keeps that true by construction rather than by both
            // call sites happening to agree.
            Err(error) => return outcome_from_publish_error(&error),
        };

        let published = match self.target.published_resources().await {
            Ok(published) => published,
            Err(error) => return outcome_from_publish_error(&error),
        };

        let declarations = match self.declared_data_sources().await {
            Ok(declarations) => declarations,
            Err(halt) => return halt.into_outcome(),
        };
        let placements = match self.recorded_placements(&declarations).await {
            Ok(placements) => placements,
            Err(halt) => return halt.into_outcome(),
        };
        let catalog = match self.runtime_catalogue().await {
            Ok(catalog) => catalog,
            Err(halt) => return halt.into_outcome(),
        };

        if catalog.is_empty() {
            return PassOutcome::Waiting {
                reason: WaitingReason::NoResources,
            };
        }

        let mut snapshot = match compose(declarations, &placements, catalog, &held, &published) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return PassOutcome::Refused {
                    reason: SafeDiagnostic::sanitise(&error.to_string()),
                }
            }
        };

        hold_unbound_catalogue(&mut snapshot, &published, &held);

        let withheld = match self.decide_registry(&mut snapshot, &placements).await {
            Ok(withheld) => withheld,
            Err(halt) => return halt.into_outcome(),
        };

        match self.offer(snapshot, withheld).await {
            Ok((report, snapshot)) => outcome_from_report(
                report,
                &held,
                PublishedRevisions {
                    tenants: Some(snapshot.tenants.revision),
                    data_sources: Some(snapshot.data_sources.revision),
                    catalog: Some(snapshot.catalog.revision),
                    registry: report.registry.map(|registry| registry.generation),
                },
            ),
            Err(error) => outcome_from_publish_error(&error),
        }
    }
}
