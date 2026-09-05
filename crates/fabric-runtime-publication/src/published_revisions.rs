//! What is currently held, one revision per document.

use crate::DocumentRevision;

/// The revision currently held for each of the three documents, as reported
/// by [`crate::RuntimePublication::current`].
///
/// `None` means no manifest has ever been published for that document — the
/// state every document starts in, and the state the publisher's own
/// presence table treats as "first publication: write, guard off" (ADR
/// 0018 part 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PublishedRevisions {
    /// The tenants document's held revision, if any.
    pub tenants: Option<DocumentRevision>,
    /// The data-sources document's held revision, if any.
    pub data_sources: Option<DocumentRevision>,
    /// The catalogue document's held revision, if any.
    pub catalog: Option<DocumentRevision>,
}
