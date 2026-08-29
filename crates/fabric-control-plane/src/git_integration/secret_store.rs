//! The Fabric instance's secret partition.
//!
//! # A capability, not a path
//!
//! Callers name a secret — `git/app-private-key` — and never a mount, a
//! prefix, or a backend. Where an instance's secrets physically live is the
//! adapter's business and a deployment's decision; the brief is explicit that
//! this domain must not acquire an opinion about it.
//!
//! # Why writing is here at all
//!
//! Reading was enough while every credential was created by a human and
//! delivered by the platform. It stopped being enough the moment the platform
//! started *generating* credential material of its own: a GitHub App's private
//! key arrives once, in the response to a manifest conversion, and if it is not
//! stored then it is gone.

use async_trait::async_trait;

/// The name of a secret within one instance's partition.
///
/// A name, never a location. `git/app-private-key` says what the value is for;
/// which mount and which prefix it ends up under is the adapter's to decide.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SecretName(String);

impl SecretName {
    /// Names a secret.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The name, for building a location or a log line.
    ///
    /// Safe to log: it is a name this code chose, and it is not the value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SecretName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A secret value.
///
/// No [`Display`](std::fmt::Display), and a [`Debug`] that prints a fixed
/// string — the same treatment
/// [`GitCredential`](https://docs.rs/) gives the credential it wraps, and for
/// the same reason: a bare `String` here is one `{:?}` away from putting a
/// GitHub App's private key into a log aggregator, and the code that leaks it
/// looks exactly like the code that does not.
#[derive(Clone)]
pub struct SecretValue(String);

impl SecretValue {
    /// Wraps a value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value, for the one caller that has to present it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretValue(redacted)")
    }
}

/// Why a secret could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretStoreError {
    /// The store could not be reached.
    #[error("the secret store is unavailable")]
    Unavailable,

    /// The platform's own credential for the store was refused.
    #[error("the platform's credential for the secret store was refused")]
    NotPermitted,

    /// The store answered with something this code cannot read.
    #[error("the secret store answered unintelligibly")]
    Malformed,

    /// This store cannot store anything.
    ///
    /// The environment-backed store is read-only: a process cannot write to
    /// the environment its orchestrator gave it. Naming that refusal is better
    /// than a write that silently succeeds and is gone at the next restart.
    #[error("this secret store is read-only")]
    ReadOnly,
}

/// Reads and writes secrets belonging to one Fabric instance.
#[async_trait]
pub trait SecretStore: Send + Sync {
    /// Reads a secret, or `None` if it has never been written.
    ///
    /// # Errors
    ///
    /// Returns [`SecretStoreError`] if the store could not be reached or
    /// refused the read. **Absence is not an error** — a platform that has not
    /// been connected yet has no key, and that is an ordinary state.
    async fn get(&self, name: &SecretName) -> Result<Option<SecretValue>, SecretStoreError>;

    /// Writes a secret, replacing any previous value.
    ///
    /// # Errors
    ///
    /// Returns [`SecretStoreError`] if the store could not be reached, refused
    /// the write, or cannot be written to at all.
    async fn put(&self, name: &SecretName, value: &SecretValue) -> Result<(), SecretStoreError>;

    /// Removes a secret. Removing one that is not there is not an error.
    ///
    /// # Errors
    ///
    /// Returns [`SecretStoreError`] if the store could not be reached or
    /// refused the removal.
    async fn delete(&self, name: &SecretName) -> Result<(), SecretStoreError>;

    /// A short description of where secrets go, for the startup log.
    ///
    /// Must name a store and never a credential for it.
    fn describe(&self) -> String;
}
