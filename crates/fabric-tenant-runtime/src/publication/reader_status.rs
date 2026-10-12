//! What a published runtime serves, and why, for readiness to report.

use std::sync::{Arc, Mutex, PoisonError};

use super::{GenerationOrigin, LastKnownGoodState, PublishedStateSource};
use crate::SourceError;

/// The generation this replica serves, how many reads were held, and its
/// last-known-good copy, kept by the refresher as it applies or holds.
///
/// [`Default`] is a status with no source: nothing served, no copy. That is
/// what a runtime built without the published reader reports.
#[derive(Default)]
pub struct ReaderStatus {
    source: Option<Arc<dyn PublishedStateSource>>,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    serving: Serving,
    held_passes: u64,
    last_hold: Option<String>,
}

/// What resolution currently reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "serving", rename_all = "snake_case")]
pub enum Serving {
    /// Nothing has been applied: the runtime is unprimed.
    #[default]
    Nothing,
    /// Documents with no publication record, each applied on its own.
    Unrecorded,
    /// One complete generation.
    Generation {
        /// Its number.
        generation: u64,
        /// Where it was read from.
        #[serde(flatten)]
        origin: GenerationOrigin,
    },
}

/// One reading of [`ReaderStatus`], as the readiness detail renders it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReaderReport {
    /// What resolution reads.
    #[serde(flatten)]
    pub serving: Serving,
    /// How many reads applied nothing and held what was served.
    pub held_passes: u64,
    /// Why the last such read was held.
    pub last_hold: Option<String>,
    /// The persisted copy.
    pub last_known_good: LastKnownGoodState,
}

impl ReaderReport {
    /// Whether the replica serves its last-known-good copy rather than a
    /// published generation: ready, but held, and worth a look.
    #[must_use]
    pub const fn holding_last_known_good(&self) -> bool {
        matches!(
            self.serving,
            Serving::Generation {
                origin: GenerationOrigin::LastKnownGood { .. },
                ..
            }
        )
    }
}

impl ReaderStatus {
    pub(crate) fn over(source: Arc<dyn PublishedStateSource>) -> Self {
        Self {
            source: Some(source),
            inner: Mutex::default(),
        }
    }

    pub(crate) fn applied(&self, serving: Serving) {
        self.lock().serving = serving;
    }

    pub(crate) fn held(&self, error: &SourceError) {
        let mut inner = self.lock();
        inner.held_passes = inner.held_passes.saturating_add(1);
        inner.last_hold = Some(error.to_string());
    }

    /// The status now.
    #[must_use]
    pub fn report(&self) -> ReaderReport {
        let last_known_good = self
            .source
            .as_ref()
            .map_or(LastKnownGoodState::Disabled, |source| source.last_known_good());
        let inner = self.lock();
        ReaderReport {
            serving: inner.serving.clone(),
            held_passes: inner.held_passes,
            last_hold: inner.last_hold.clone(),
            last_known_good,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
