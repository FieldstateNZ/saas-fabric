//! What one publication pass does: read the platform's inputs, compose a
//! snapshot, and offer it -- the body [`crate::RuntimePublisher::publish_once`]
//! runs once its re-entry guard has let it through.

use fabric_runtime_publication::{DocumentOutcome, PublicationReport, PublishedRevisions};

use crate::publication::outcome::{PassOutcome, WaitingReason};
use crate::publication::protocol::publish_with_retry;
use crate::publication::snapshot::compose;
use crate::publication::RuntimePublisher;
use crate::SafeDiagnostic;

/// Where reading an input lands, before there is a [`PassOutcome`] to
/// build: a coherence problem this platform itself wrote, or a catalogue
/// conflict, is [`Self::Refused`]; a transport failure reaching any input
/// is [`Self::Failed`]. Built by `publication::reads`, consumed here.
pub(super) enum Halt {
    Refused(SafeDiagnostic),
    Failed(SafeDiagnostic),
}

impl Halt {
    fn into_outcome(self) -> PassOutcome {
        match self {
            Self::Refused(reason) => PassOutcome::Refused { reason },
            Self::Failed(detail) => PassOutcome::Failed { detail },
        }
    }
}

/// `Unchanged` if every document settled unchanged, `Published` with the
/// report's own outcomes otherwise.
fn outcome_from_report(report: PublicationReport, revisions: PublishedRevisions) -> PassOutcome {
    let PublicationReport {
        tenants,
        data_sources,
        catalog,
    } = report;

    if tenants == DocumentOutcome::Unchanged
        && data_sources == DocumentOutcome::Unchanged
        && catalog == DocumentOutcome::Unchanged
    {
        PassOutcome::Unchanged { revisions }
    } else {
        PassOutcome::Published {
            tenants,
            data_sources,
            catalog,
            revisions,
        }
    }
}

impl RuntimePublisher {
    /// Runs one pass: read, compose, offer -- without the re-entry guard
    /// [`crate::RuntimePublisher::publish_once`] holds around this call.
    pub(super) async fn run_pass(&self) -> PassOutcome {
        let held = match self.target.current().await {
            Ok(held) => held,
            Err(error) => {
                return PassOutcome::Failed {
                    detail: SafeDiagnostic::sanitise(&error.to_string()),
                }
            }
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

        let snapshot = match compose(declarations, &placements, catalog, &held) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return PassOutcome::Refused {
                    reason: SafeDiagnostic::sanitise(&error.to_string()),
                }
            }
        };

        match publish_with_retry(self.target.as_ref(), snapshot).await {
            Ok((report, snapshot)) => outcome_from_report(
                report,
                PublishedRevisions {
                    tenants: Some(snapshot.tenants.revision),
                    data_sources: Some(snapshot.data_sources.revision),
                    catalog: Some(snapshot.catalog.revision),
                },
            ),
            Err(error) => PassOutcome::Refused {
                reason: SafeDiagnostic::sanitise(&error.to_string()),
            },
        }
    }
}
