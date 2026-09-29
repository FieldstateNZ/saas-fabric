//! What a client for one registry is built from: where it is, which realm
//! it may take tokens from, and the credential it presents for which
//! repositories.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use fabric_component::Repository;

use crate::git_integration::SecretValue;
use crate::registries::{RegistryHost, RegistryKind};

/// A registry credential: a username, and a token nothing can print.
#[derive(Debug, Clone)]
pub struct RegistryCredential {
    /// The registry account.
    username: String,

    /// Its long-lived token. `Debug` prints `SecretValue(redacted)`.
    token: SecretValue,

    /// Whether its realm refused it, shared by every client built with it.
    refused: Arc<AtomicBool>,
}

impl RegistryCredential {
    /// A credential, not refused.
    #[must_use]
    pub fn new(username: impl Into<String>, token: SecretValue) -> Self {
        Self {
            username: username.into(),
            token,
            refused: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The same credential, carrying `mark`: the one every client built with
    /// this stored credential shares.
    ///
    /// # Why a mark travels with the credential
    ///
    /// A change that rebuilds a registry's client for the same credential —
    /// adding or removing a repository — must neither clear a refusal nor
    /// leave one on a client that is thrown away, or the next sweep presents
    /// a credential its realm already refused (ADR 0026 section 5). So the
    /// service holds one mark per stored credential and hands it to every
    /// client built from it; a new credential, or a restart, starts with a
    /// fresh one.
    #[must_use]
    pub fn marked_by(mut self, mark: Arc<AtomicBool>) -> Self {
        self.refused = mark;
        self
    }

    /// Its refusal mark, for the adapter that presents it.
    #[must_use]
    pub fn refusal_mark(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.refused)
    }

    /// The registry account.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// The token, for the one adapter that presents it.
    #[must_use]
    pub fn token(&self) -> &SecretValue {
        &self.token
    }
}

/// Which realm a client may take tokens from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealmOrigin {
    /// A registry being proven for registration: the realm its first
    /// challenge names is followed and reported, to be recorded.
    Follow,

    /// A registered registry: the realm origin recorded when it was proven,
    /// or `None` if it named none. A `ghcr` or `dockerHub` registry's realm
    /// is its kind's whatever this says.
    Recorded(Option<String>),
}

/// Everything a client for one registry is built from.
#[derive(Debug, Clone)]
pub struct RegistryConnection {
    /// What it is known by.
    pub host: RegistryHost,

    /// Its kind.
    pub kind: RegistryKind,

    /// Where it is served.
    pub endpoint: String,

    /// Which realm it may take tokens from.
    pub realm: RealmOrigin,

    /// Its credential, when one is held.
    pub credential: Option<RegistryCredential>,

    /// The repositories the credential is presented for: every other
    /// repository on the host is read anonymously.
    pub repositories: Vec<Repository>,
}
