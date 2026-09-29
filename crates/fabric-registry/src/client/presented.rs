//! The credential a registry holds: for which repositories it is presented,
//! whether its realm has refused it, and how it is attached to a request.
//!
//! # Why a refusal is remembered, and nothing else is
//!
//! A realm that refuses a credential will refuse it again, and a sweep asking
//! every minute would walk an account into a lockout. So a refusal marks it,
//! and every request that would present it fails with
//! [`RegistryError::Denied`] without contacting anything (ADR 0026 section
//! 5). Anonymous requests are unaffected. The mark belongs to the
//! credential, shared by every client built with it through
//! [`Credential::sharing_refusal`]: rebuilding a client for the same
//! credential keeps it, while a new credential — like a restart — starts
//! unmarked and is presented once more.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use fabric_platform_management::RegistryError;

use crate::client::scope::Scope;
use crate::client::OciRegistry;
use crate::settings::{Credential, RegistrySecret};

/// A credential, as a client holds it.
pub(crate) struct Presented {
    /// Who the secret belongs to.
    pub(crate) username: String,

    /// The secret.
    pub(crate) secret: RegistrySecret,

    /// The repository paths it is presented for, the naming host stripped.
    paths: BTreeSet<String>,

    /// Whether its realm refused it, shared with every client built with
    /// the same credential.
    refused: Arc<AtomicBool>,
}

impl Presented {
    /// Holds `credential`, naming its repositories as API paths.
    pub(crate) fn new(credential: Credential, naming_host: &str) -> Self {
        let prefix = format!("{naming_host}/");
        let paths = credential
            .repositories
            .iter()
            .map(|repository| repository.strip_prefix(&prefix).unwrap_or(repository).to_owned())
            .collect();
        Self {
            username: credential.username,
            secret: credential.secret,
            paths,
            refused: credential.refused,
        }
    }
}

impl OciRegistry {
    /// Whether this client's realm refused its credential. `false` when it
    /// holds none, and on every new client.
    #[must_use]
    pub fn credential_refused(&self) -> bool {
        self.credential
            .as_ref()
            .is_some_and(|credential| credential.refused.load(Ordering::SeqCst))
    }

    /// Whether a request in `scope` presents the credential: proving the
    /// registry does whenever one is held, and a repository only when the
    /// credential was registered for it.
    pub(super) fn presents(&self, scope: Scope<'_>) -> bool {
        match (&self.credential, scope) {
            (None, _) => false,
            (Some(_), Scope::Registry) => true,
            (Some(credential), Scope::Repository(path)) => credential.paths.contains(path),
        }
    }

    /// Fails without contacting anything if the credential was refused.
    ///
    /// # Errors
    ///
    /// [`RegistryError::Denied`] once the realm has refused it.
    pub(super) fn not_refused(&self, operation: &str) -> Result<(), RegistryError> {
        if self.credential_refused() {
            return Err(RegistryError::Denied {
                detail: format!(
                    "{operation}: the realm refused this registry's credential, which is not presented again until it is replaced"
                ),
            });
        }
        Ok(())
    }

    /// Marks the credential refused, and says so.
    pub(super) fn refuse(&self, operation: &str) -> RegistryError {
        if let Some(credential) = &self.credential {
            credential.refused.store(true, Ordering::SeqCst);
        }
        RegistryError::Denied {
            detail: format!("{operation}: the realm refused this registry's credential"),
        }
    }
}
