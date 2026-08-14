//! A source held in memory, for tests and local development.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::resource::{RegistryResource, ResourceSource};
use crate::SourceError;

/// Serves resources from memory.
///
/// Also the reference for what a source must do: hand back the *complete*
/// current set every time, never a delta.
///
/// [`Self::fail_next`] exists so tests can exercise the path that matters most
/// — that a load failure leaves the previous snapshot serving rather than
/// clearing it — and [`Self::loads`] so they can prove that path was actually
/// taken rather than assuming it.
pub struct InMemorySource<T> {
    state: Mutex<State<T>>,
}

/// The mutable interior, in one place so the lock covers it all.
struct State<T> {
    resources: Vec<T>,
    fail_next: bool,
    loads: usize,
}

impl<T> InMemorySource<T> {
    /// Builds a source serving the given resources.
    #[must_use]
    pub fn new(resources: Vec<T>) -> Self {
        Self {
            state: Mutex::new(State {
                resources,
                fail_next: false,
                loads: 0,
            }),
        }
    }

    /// Builds an empty source.
    #[must_use]
    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    /// Replaces what the source will serve from now on.
    ///
    /// # Panics
    ///
    /// Never in practice: a poisoned lock means another thread panicked while
    /// holding it, which in a test is already the failure.
    pub fn set(&self, resources: Vec<T>) {
        self.lock().resources = resources;
    }

    /// Makes the next [`ResourceSource::load`] fail.
    ///
    /// # Panics
    ///
    /// See [`Self::set`].
    pub fn fail_next(&self) {
        self.lock().fail_next = true;
    }

    /// How many times [`ResourceSource::load`] has been called, successfully or
    /// not.
    ///
    /// A test that asserts the registry is unchanged after a failed refresh is
    /// asserting nothing unless the refresh actually happened — the registry
    /// was unchanged before the test started too. This is what lets such a test
    /// prove its own precondition instead of assuming a background task got a
    /// turn.
    ///
    /// # Panics
    ///
    /// See [`Self::set`].
    #[must_use]
    pub fn loads(&self) -> usize {
        self.lock().loads
    }

    /// Takes the lock, recovering from poisoning.
    fn lock(&self) -> std::sync::MutexGuard<'_, State<T>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[async_trait]
impl<T: RegistryResource> ResourceSource<T> for InMemorySource<T> {
    async fn load(&self) -> Result<Vec<T>, SourceError> {
        let mut state = self.lock();
        state.loads += 1;

        if std::mem::take(&mut state.fail_next) {
            return Err(SourceError::Malformed {
                origin: "in-memory".to_owned(),
                detail: "injected failure".to_owned(),
            });
        }

        Ok(state.resources.clone())
    }

    fn describe(&self) -> String {
        "in-memory".to_owned()
    }
}
