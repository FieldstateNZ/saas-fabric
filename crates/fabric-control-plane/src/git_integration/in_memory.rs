//! Stores that keep everything in this process and lose it on restart.
//!
//! **Development only**, and the same trade the in-memory client repository
//! makes: the control plane must be runnable without a cluster (§22), and a
//! fake that skipped the port's actual semantics would make every test of them
//! meaningless. These keep the semantics — absence is not an error, a write
//! replaces, a delete of nothing succeeds — and lose the contents.

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::git_integration::{
    GitIntegration, IntegrationStore, IntegrationStoreError, SecretName, SecretStore, SecretStoreError,
    SecretValue,
};

/// Secrets held in this process.
#[derive(Default)]
pub struct InMemorySecretStore {
    /// The secrets, by name.
    held: Mutex<BTreeMap<SecretName, SecretValue>>,
}

impl InMemorySecretStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SecretStore for InMemorySecretStore {
    async fn get(&self, name: &SecretName) -> Result<Option<SecretValue>, SecretStoreError> {
        Ok(lock(&self.held).get(name).cloned())
    }

    async fn put(&self, name: &SecretName, value: &SecretValue) -> Result<(), SecretStoreError> {
        lock(&self.held).insert(name.clone(), value.clone());
        Ok(())
    }

    async fn delete(&self, name: &SecretName) -> Result<(), SecretStoreError> {
        lock(&self.held).remove(name);
        Ok(())
    }

    fn describe(&self) -> String {
        "an in-memory secret store; contents are lost when this process stops".to_owned()
    }
}

/// An integration record held in this process.
#[derive(Default)]
pub struct InMemoryIntegrationStore {
    /// The record, if one has been saved.
    held: Mutex<Option<GitIntegration>>,
}

impl InMemoryIntegrationStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl IntegrationStore for InMemoryIntegrationStore {
    async fn load(&self) -> Result<Option<GitIntegration>, IntegrationStoreError> {
        Ok(lock(&self.held).clone())
    }

    async fn save(&self, integration: &GitIntegration) -> Result<(), IntegrationStoreError> {
        *lock(&self.held) = Some(integration.clone());
        Ok(())
    }

    async fn clear(&self) -> Result<(), IntegrationStoreError> {
        *lock(&self.held) = None;
        Ok(())
    }
}

/// Takes a lock, treating a poisoned one as usable.
///
/// Nothing behind these locks is invariant-bearing — a map and an option — so
/// a panic elsewhere should not make the store permanently unusable.
fn lock<T>(held: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    held.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
