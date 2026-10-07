//! Turning the report a completed offer returned into what the pass did.

use fabric_runtime_publication::{DocumentOutcome, DocumentRevision, PublicationReport, PublishedRevisions};

use crate::publication::outcome::PassOutcome;

/// `Unchanged` if every document settled unchanged, `Published` with the
/// report's own outcomes otherwise.
///
/// The report is the last offer's. When an earlier offer in the same pass
/// was interrupted part-way, documents it already wrote settle as unchanged
/// on the offer that completed; comparing against what was held when the
/// pass began is what still reports them as written by this pass.
pub(super) fn outcome_from_report(
    report: PublicationReport,
    held: &PublishedRevisions,
    revisions: PublishedRevisions,
) -> PassOutcome {
    let tenants = written_this_pass(report.tenants, held.tenants, revisions.tenants);
    let data_sources = written_this_pass(report.data_sources, held.data_sources, revisions.data_sources);
    let catalog = written_this_pass(report.catalog, held.catalog, revisions.catalog);

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

fn written_this_pass(
    reported: DocumentOutcome,
    held: Option<DocumentRevision>,
    accepted: Option<DocumentRevision>,
) -> DocumentOutcome {
    if held == accepted {
        reported
    } else {
        DocumentOutcome::Written
    }
}
