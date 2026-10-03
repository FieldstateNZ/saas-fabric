//! A registry store that keeps its records in this process.
//!
//! **Development and tests only**, the same trade the in-memory integration
//! store makes: the semantics are kept — nothing recorded is an empty set, a
//! save replaces the whole set — and the contents are lost on restart.

use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;

use crate::registries::{RegistryRecord, RegistryStore, RegistryStoreError};

/// Registry records held in this process.
#[derive(Default)]
pub struct InMemoryRegistryStore {
    /// The record set, `None` until something is saved.
    held: Mutex<Option<Vec<RegistryRecord>>>,
}

impl InMemoryRegistryStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl RegistryStore for InMemoryRegistryStore {
    async fn load(&self) -> Result<Vec<RegistryRecord>, RegistryStoreError> {
        Ok(self
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_default())
    }

    async fn save(&self, records: &[RegistryRecord]) -> Result<(), RegistryStoreError> {
        *self.held.lock().unwrap_or_else(PoisonError::into_inner) = Some(records.to_vec());
        Ok(())
    }
}
