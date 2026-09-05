//! Everything one call to [`crate::RuntimePublication::publish`] offers.

use crate::{CatalogDocument, DataSourceDocument, DocumentRevision, TenantBindingDocument};

/// Whether a publication that would take a currently non-empty document to
/// empty is intended, or should be refused.
///
/// ADR 0018 part 6: a publication that would empty a non-empty document is
/// refused unless the caller says so explicitly, right here — never as a
/// flag on the port itself, so the intent travels with the exact snapshot it
/// applies to rather than as separate, easy-to-forget state. This is the
/// producer-side analogue of the consumer's `UnusableFirstLoad`: that guard
/// stops an empty result from being mistaken for a legitimate *first* state;
/// this one stops an empty result from being mistaken for a legitimate
/// *change*. What it prevents is concrete: a scheduled publication whose
/// input query returned zero rows would otherwise deprovision every tenant
/// to a 403, silently, on the next sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Emptying {
    /// Refuse the publication if it would take this document from non-empty
    /// to empty. The safe default every caller gets unless it deliberately
    /// opts out.
    #[default]
    NotIntended,
    /// This document may legitimately go from non-empty to empty — a
    /// deliberate deprovisioning, stated explicitly rather than inferred.
    Intended,
}

/// One document's incoming state: what is being offered, at what revision,
/// and whether taking that document to empty is intended.
#[derive(Debug, Clone)]
pub struct DocumentInput<T> {
    /// The revision the caller asserts this publication moves the document
    /// to. The publisher never generates this — see the crate's module docs
    /// for why a caller-stated revision is what makes a stale-revision
    /// refusal a reachable state at all.
    pub revision: DocumentRevision,
    /// The document's complete contents. Always a full replacement — there
    /// is no partial-update path, because `ResourceRegistry::apply_all`
    /// treats a missing entry as a removal.
    pub payload: T,
    /// Whether taking this document from non-empty to empty is intended.
    pub emptying: Emptying,
}

impl<T> DocumentInput<T> {
    /// Builds one document's input, defaulting to
    /// [`Emptying::NotIntended`] — the safe default every caller gets unless
    /// it deliberately opts into deprovisioning.
    #[must_use]
    pub fn new(revision: DocumentRevision, payload: T) -> Self {
        Self {
            revision,
            payload,
            emptying: Emptying::NotIntended,
        }
    }

    /// Marks this document's emptying as intended.
    #[must_use]
    pub fn emptying_intended(mut self) -> Self {
        self.emptying = Emptying::Intended;
        self
    }
}

/// Everything one call to [`crate::RuntimePublication::publish`] offers: all
/// three documents, every time.
///
/// The port takes all three on every call rather than an `Option` per
/// document — independence between documents comes from their separately
/// advancing revisions, not from a caller being able to omit one. An
/// `Option` per document would let a caller publish a tenants document
/// referencing a DataSource it never published in the same breath, and the
/// referential check the publisher runs would have nothing to check that
/// binding against.
#[derive(Debug, Clone)]
pub struct RuntimeSnapshot {
    /// Every tenant's complete runtime binding.
    pub tenants: DocumentInput<Vec<TenantBindingDocument>>,
    /// Every configured DataSource.
    pub data_sources: DocumentInput<Vec<DataSourceDocument>>,
    /// The whole resource catalogue.
    ///
    /// Can never legitimately be empty — see
    /// [`crate::PublicationError::EmptyCatalogue`]. This document still
    /// carries an [`Emptying`] intent for the same reason
    /// [`RuntimeSnapshot`] carries three documents rather than three
    /// `Option`s: a uniform shape. The intent is simply never enough to let
    /// an empty catalogue through — decision 6 has no threshold and no
    /// override.
    pub catalog: DocumentInput<CatalogDocument>,
}
