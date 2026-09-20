//! What [`FakePublication`](super::FakePublication) currently holds, and
//! the one-shot script that overrides its next verdict.

use fabric_runtime_publication::{
    DocumentKind, DocumentOutcome, DocumentRevision, PublicationError, PublishedRevisions,
};

/// What [`FakePublication`](super::FakePublication) answers for one
/// document. The plain variants answer once, then clear; `Divergent {
/// persist: true }` keeps answering, for a test that exhausts every retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scripted {
    Divergent { persist: bool },
    Stale,
}

/// One scripted slot per document -- `DocumentKind` has no `Ord`/`Hash`, so
/// this is `PublishedRevisions`'s shape rather than a map.
#[derive(Default)]
pub(super) struct Script {
    tenants: Option<Scripted>,
    data_sources: Option<Scripted>,
    catalog: Option<Scripted>,
}

impl Script {
    pub(super) fn slot_mut(&mut self, document: DocumentKind) -> &mut Option<Scripted> {
        match document {
            DocumentKind::Tenants => &mut self.tenants,
            DocumentKind::DataSources => &mut self.data_sources,
            DocumentKind::Catalog => &mut self.catalog,
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    pub(super) held: PublishedRevisions,
    pub(super) scripted: Script,
    pub(super) writes: Vec<DocumentKind>,
}

impl State {
    fn held_mut(&mut self, document: DocumentKind) -> &mut Option<DocumentRevision> {
        match document {
            DocumentKind::Tenants => &mut self.held.tenants,
            DocumentKind::DataSources => &mut self.held.data_sources,
            DocumentKind::Catalog => &mut self.held.catalog,
        }
    }

    /// A scripted refusal for `document`, consumed unless it persists.
    pub(super) fn scripted_refusal(
        &mut self,
        document: DocumentKind,
        offered: DocumentRevision,
    ) -> Option<PublicationError> {
        let scripted = (*self.scripted.slot_mut(document))?;
        if !matches!(scripted, Scripted::Divergent { persist: true }) {
            *self.scripted.slot_mut(document) = None;
        }
        Some(match scripted {
            Scripted::Divergent { .. } => PublicationError::DivergentPayload {
                document,
                revision: offered,
            },
            Scripted::Stale => PublicationError::StaleRevision {
                document,
                held: offered,
                offered,
            },
        })
    }

    /// Unchanged if already held at `offered`; written (and now held)
    /// otherwise.
    pub(super) fn settle(&mut self, document: DocumentKind, offered: DocumentRevision) -> DocumentOutcome {
        let held = self.held_mut(document);
        if *held == Some(offered) {
            DocumentOutcome::Unchanged
        } else {
            *held = Some(offered);
            self.writes.push(document);
            DocumentOutcome::Written
        }
    }
}
