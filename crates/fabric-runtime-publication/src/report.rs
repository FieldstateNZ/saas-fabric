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
/// resolved without error — a refused publication never produces one of
/// these; it produces a [`crate::PublicationError`] instead, and writes
/// nothing at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicationReport {
    /// What happened to the tenants document.
    pub tenants: DocumentOutcome,
    /// What happened to the data-sources document.
    pub data_sources: DocumentOutcome,
    /// What happened to the catalogue document.
    pub catalog: DocumentOutcome,
}
