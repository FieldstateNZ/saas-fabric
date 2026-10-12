//! An in-memory fake [`RuntimePublication`], for tests that need a real
//! offer-and-advance conversation without a filesystem.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md): a
//! test-only fake whose one `impl RuntimePublication` must script every
//! refusal the protocol retries, so it stays one file.

mod held;
mod inspection;

use std::sync::Mutex;

use fabric_runtime_publication::{
    DocumentKind, DocumentOutcome, PublicationCompleteness, PublicationError, PublicationReport,
    PublishedResources, PublishedRevisions, RegistryGeneration, RegistryOutcome, RuntimePublication,
    RuntimeSnapshot,
};

use held::{Scripted, State};
use inspection::fake_stamp;

/// Records what it was asked to write, and can be told in advance to answer
/// [`PublicationError::DivergentPayload`] or
/// [`PublicationError::StaleRevision`] for a named document, or to fail one
/// document's write part-way with [`PublicationError::Unwritable`].
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

    /// Fails the write of `document` once, after every document before it
    /// in write order has been written.
    pub(crate) fn script_interrupted(&self, document: DocumentKind) {
        self.script(document, Scripted::Interrupted { persist: false });
    }

    /// [`Self::script_interrupted`], on every offer.
    pub(crate) fn script_always_interrupted(&self, document: DocumentKind) {
        self.script(document, Scripted::Interrupted { persist: true });
    }

    /// Drops whatever is scripted for `document`.
    pub(crate) fn unscript(&self, document: DocumentKind) {
        *self.lock().scripted.slot_mut(document) = None;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn script(&self, document: DocumentKind, scripted: Scripted) {
        *self.lock().scripted.slot_mut(document) = Some(scripted);
    }

    /// Every document this fake actually wrote, in call order.
    pub(crate) fn writes(&self) -> Vec<DocumentKind> {
        self.lock().writes.clone()
    }

    /// How many times `publish` was called at all -- written, settled
    /// unchanged, or refused. What pins `protocol::MAX_RETRIES` from above:
    /// one initial offer plus at most one retry per document in a
    /// three-document snapshot is four, never more.
    pub(crate) fn publish_attempts(&self) -> usize {
        self.lock().publish_attempts
    }
}

#[async_trait::async_trait]
impl RuntimePublication for FakePublication {
    async fn current(&self) -> Result<PublishedRevisions, PublicationError> {
        Ok(self.lock().held)
    }

    /// Nothing: this fake tracks revisions, not content.
    async fn published_resources(&self) -> Result<PublishedResources, PublicationError> {
        Ok(PublishedResources::default())
    }

    async fn publish(&self, snapshot: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
        let mut state = self.lock();
        state.publish_attempts += 1;

        for (document, offered) in [
            (DocumentKind::DataSources, snapshot.data_sources.revision),
            (DocumentKind::Catalog, snapshot.catalog.revision),
            (DocumentKind::Tenants, snapshot.tenants.revision),
        ] {
            if let Some(refusal) = state.scripted_refusal(document, offered) {
                return Err(refusal);
            }
        }

        let [data_sources, catalog, tenants] = state.settle_offer([
            (DocumentKind::DataSources, snapshot.data_sources.revision),
            (DocumentKind::Catalog, snapshot.catalog.revision),
            (DocumentKind::Tenants, snapshot.tenants.revision),
        ])?;

        Ok(PublicationReport {
            tenants,
            data_sources,
            catalog,
            registry: snapshot.registry.as_ref().map(|_| RegistryOutcome {
                outcome: DocumentOutcome::Written,
                generation: RegistryGeneration::FIRST,
            }),
            record: fake_stamp(state.publish_attempts),
        })
    }

    async fn inspect(&self) -> PublicationCompleteness {
        self.lock().inspect()
    }

    fn describe(&self) -> String {
        "fake".to_owned()
    }
}
