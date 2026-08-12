//! A binding source held in memory, for tests and local development.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::{BindingSource, BindingSourceError, TenantRuntimeBinding};

/// Serves bindings from memory.
///
/// For tests and local development. It also serves as the reference for what a
/// source must do: hand back the *complete* current set every time, never a
/// delta.
///
/// [`Self::fail_next`] exists so tests can exercise the path that matters most
/// — that a load failure leaves the previous snapshot serving rather than
/// clearing it.
pub struct InMemoryBindingSource {
    state: Mutex<State>,
}

/// The mutable interior, kept in one place so the lock covers it all.
struct State {
    bindings: Vec<TenantRuntimeBinding>,
    fail_next: bool,
}

impl InMemoryBindingSource {
    /// Builds a source serving the given bindings.
    #[must_use]
    pub fn new(bindings: Vec<TenantRuntimeBinding>) -> Self {
        Self {
            state: Mutex::new(State {
                bindings,
                fail_next: false,
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
    /// If the lock is poisoned, which only happens if another thread panicked
    /// while holding it — in a test, that is already the failure.
    pub fn set(&self, bindings: Vec<TenantRuntimeBinding>) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.bindings = bindings;
    }

    /// Makes the next [`BindingSource::load`] fail.
    ///
    /// # Panics
    ///
    /// If the lock is poisoned. See [`Self::set`].
    pub fn fail_next(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.fail_next = true;
    }
}

#[async_trait]
impl BindingSource for InMemoryBindingSource {
    async fn load(&self) -> Result<Vec<TenantRuntimeBinding>, BindingSourceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if std::mem::take(&mut state.fail_next) {
            return Err(BindingSourceError::Malformed {
                origin: "in-memory".to_owned(),
                detail: "injected failure".to_owned(),
            });
        }

        Ok(state.bindings.clone())
    }

    fn describe(&self) -> String {
        "in-memory".to_owned()
    }
}
