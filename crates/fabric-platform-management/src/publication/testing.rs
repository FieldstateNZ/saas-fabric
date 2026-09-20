//! An in-memory fake [`RuntimePublication`], for tests that need a real
//! offer-and-advance conversation without a filesystem.

mod held;

use std::sync::Mutex;

use fabric_runtime_publication::{
    DocumentKind, PublicationError, PublicationReport, PublishedRevisions, RuntimePublication,
    RuntimeSnapshot,
};

use held::{Scripted, State};

/// Records what it was asked to write, and can be told in advance to answer
/// [`PublicationError::DivergentPayload`] or
/// [`PublicationError::StaleRevision`] for a named document.
#[derive(Default)]
pub(crate) struct FakePublication {
    state: Mutex<State>,
}

impl FakePublication {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Answers `DivergentPayload` the next time `document` is offered.
    pub(crate) fn script_divergent(&self, document: DocumentKind) {
        self.script(document, Scripted::Divergent { persist: false });
    }

    /// [`Self::script_divergent`], but keeps answering it -- for a test
    /// that exhausts the protocol's retry budget.
    pub(crate) fn script_always_divergent(&self, document: DocumentKind) {
        self.script(document, Scripted::Divergent { persist: true });
    }

    /// [`Self::script_divergent`]'s sibling for `StaleRevision`.
    pub(crate) fn script_stale(&self, document: DocumentKind) {
        self.script(document, Scripted::Stale);
    }

    fn script(&self, document: DocumentKind, scripted: Scripted) {
        if let Ok(mut state) = self.state.lock() {
            *state.scripted.slot_mut(document) = Some(scripted);
        }
    }

    /// Every document this fake actually wrote, in call order.
    pub(crate) fn writes(&self) -> Vec<DocumentKind> {
        self.state
            .lock()
            .map(|state| state.writes.clone())
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl RuntimePublication for FakePublication {
    async fn current(&self) -> Result<PublishedRevisions, PublicationError> {
        Ok(self
            .state
            .lock()
            .map_or_else(|poisoned| poisoned.into_inner().held, |state| state.held))
    }

    async fn publish(&self, snapshot: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        for (document, offered) in [
            (DocumentKind::DataSources, snapshot.data_sources.revision),
            (DocumentKind::Catalog, snapshot.catalog.revision),
            (DocumentKind::Tenants, snapshot.tenants.revision),
        ] {
            if let Some(refusal) = state.scripted_refusal(document, offered) {
                return Err(refusal);
            }
        }

        Ok(PublicationReport {
            data_sources: state.settle(DocumentKind::DataSources, snapshot.data_sources.revision),
            catalog: state.settle(DocumentKind::Catalog, snapshot.catalog.revision),
            tenants: state.settle(DocumentKind::Tenants, snapshot.tenants.revision),
        })
    }

    fn describe(&self) -> String {
        "fake".to_owned()
    }
}
