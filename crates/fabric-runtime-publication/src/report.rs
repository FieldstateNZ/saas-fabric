//! What a publication call actually did to each document.

use crate::verdict::Verdict;

/// Whether one document was rewritten by a publication call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentOutcome {
    /// The payload was rewritten, and its manifest advanced to the new
    /// revision.
    Written,
    /// Nothing changed. The publication was accepted, but the revision and
    /// bytes both matched what was already held, so nothing — not even the
    /// manifest — was rewritten.
    Unchanged,
}

impl From<Verdict> for DocumentOutcome {
    fn from(verdict: Verdict) -> Self {
        match verdict {
            Verdict::Write => Self::Written,
            Verdict::Unchanged => Self::Unchanged,
        }
    }
}

/// What [`crate::RuntimePublication::publish`] did to each of the three
/// documents.
///
/// Only ever returned once every document's verdict has already been
/// resolved without error — a publication refused by validation or by a
/// document's verdict never produces one of these; it produces a
/// [`crate::PublicationError`] instead, and writes nothing at all.
///
/// # A `PublicationReport` is not proof the whole call succeeded cleanly
///
/// It is proof every document's verdict was resolved and applied — but
/// "applied" can still mean a partial cross-document write happened on an
/// *earlier* call. `publish` writes data sources, then the catalogue, then
/// tenants (ADR 0018 part 3), each payload before its manifest; an I/O
/// failure between two of those writes surfaces as
/// [`crate::PublicationError::Unwritable`], not as a `PublicationReport`,
/// but it leaves whichever documents already landed exactly as written. The
/// next call — even the very same snapshot, at the same revisions — is what
/// finishes the job: documents already on disk resolve to
/// [`DocumentOutcome::Unchanged`], and the rest resolve to
/// [`DocumentOutcome::Written`], which is why
/// `a_publication_that_failed_between_documents_is_completed_by_the_next_one`
/// (`tests/filesystem_runtime_publication.rs`) asserts exactly that report
/// shape. A caller that always publishes at `current() + 1` converges the
/// same way without needing to know a prior call was ever interrupted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicationReport {
    /// What happened to the tenants document.
    pub tenants: DocumentOutcome,
    /// What happened to the data-sources document.
    pub data_sources: DocumentOutcome,
    /// What happened to the catalogue document.
    pub catalog: DocumentOutcome,
}
