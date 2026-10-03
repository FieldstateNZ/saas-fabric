//! A registry credential: a username, a secret nobody can print, the
//! repositories it is presented for, and whether its realm refused it.
//!
//! Kept in one file because the secret and the credential that carries it
//! are one rule — what may be printed, and where the value goes.

use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// A registry credential's secret: a long-lived token a person typed once.
///
/// # Why no `Display`, and a `Debug` that prints nothing
///
/// A secret value never reaches a log, an audit event, an error detail or a
/// response. The easiest way to put one there is `{}` or `{:?}` on a struct
/// that holds it, so neither can: `Debug` prints `RegistrySecret(redacted)`,
/// and there is no `Display`. The one way to the value is
/// `expose`, crate-private, called only where it becomes an
/// `Authorization` header.
#[derive(Clone)]
pub struct RegistrySecret(String);

impl RegistrySecret {
    /// Wraps a secret.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value, for the one place it is sent: an `Authorization` header.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for RegistrySecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RegistrySecret(redacted)")
    }
}

/// A username and a secret, and the repositories they are presented for.
///
/// A credential is presented only for the repositories registered under its
/// registry; every other repository on that host is read anonymously (ADR
/// 0026 section 5).
#[derive(Debug, Clone)]
pub struct Credential {
    /// Who the secret belongs to.
    pub(crate) username: String,

    /// The secret.
    pub(crate) secret: RegistrySecret,

    /// Repositories, as registered: `ghcr.io/org/app` or `org/app`.
    pub(crate) repositories: BTreeSet<String>,

    /// Whether its realm refused it: shared by every client built with it.
    pub(crate) refused: Arc<AtomicBool>,
}

impl Credential {
    /// A credential for `repositories`.
    ///
    /// # Errors
    ///
    /// A message, naming the field and never its value, if the username is
    /// empty or holds a `:` — which HTTP `Basic` cannot carry — or if the
    /// secret is empty.
    pub fn new<I, R>(
        username: impl Into<String>,
        secret: RegistrySecret,
        repositories: I,
    ) -> Result<Self, String>
    where
        I: IntoIterator<Item = R>,
        R: Into<String>,
    {
        let username = username.into();
        if username.trim().is_empty() || username.contains(':') {
            return Err("registry: the credential's username is empty or holds a ':'".to_owned());
        }
        if secret.expose().trim().is_empty() {
            return Err("registry: the credential's secret is empty".to_owned());
        }

        Ok(Self {
            username,
            secret,
            repositories: repositories.into_iter().map(Into::into).collect(),
            refused: Arc::new(AtomicBool::new(false)),
        })
    }

    /// The same credential, marked refused whenever `mark` is, and marking
    /// it when its realm refuses.
    ///
    /// # Why a mark is shared
    ///
    /// A change that rebuilds a client without replacing the credential —
    /// adding or removing a repository — must not clear a refusal, or the
    /// next sweep presents a credential its realm already refused (ADR 0026
    /// section 5). And a repository proven through a client built for the
    /// change must mark the client discovery reads through. So the one who
    /// holds the credential holds its mark, and hands the same one to every
    /// client built from it; a new credential, or a restart, starts with a
    /// fresh one.
    #[must_use]
    pub fn sharing_refusal(mut self, mark: Arc<AtomicBool>) -> Self {
        self.refused = mark;
        self
    }

    /// Who the secret belongs to.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// The repositories it is presented for.
    #[must_use]
    pub fn repositories(&self) -> &BTreeSet<String> {
        &self.repositories
    }
}
