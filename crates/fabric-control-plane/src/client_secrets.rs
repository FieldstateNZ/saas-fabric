//! Managing one client's secrets, inside that client's boundary and nowhere
//! else.
//!
//! # The shape of the whole thing
//!
//! ```text
//! operator            authenticated at the control plane's boundary
//!     ↓
//! client id           the operator names a client and a path within it
//!     ↓
//! desired state       the client's declared secret boundary
//!     ↓
//! ClientSecrets       this port, in the platform's words
//!     ↓
//! fabric-openbao      the only place the store's protocol exists
//! ```
//!
//! **A caller supplies a client and a path. Nothing else.** No boundary, no
//! mount, no address, no token, no policy — all of those are trusted platform
//! state, and a request that could name its own boundary is a request that can
//! read another client's secrets.
//!
//! # Why the store's versioning is kept rather than flattened
//!
//! An operator opens version 7, somebody else writes version 8, and the first
//! operator saves. Flattening versions away makes that a silent overwrite.
//! Keeping them makes it a refusal the operator can see and act on, which is
//! the entire reason a console shows a version number at all.
//!
//! # This is not a store browser
//!
//! The product concept is *manage Acme's secrets*, not *here is an embedded
//! admin console*. Nothing here exposes the store's own vocabulary, so a
//! client whose secrets one day live somewhere else needs a new adapter and
//! not a new screen.

mod errors;
mod path;
#[cfg(test)]
mod path_tests;
mod values;

use async_trait::async_trait;
use fabric_client_model::SecretNamespace;

pub use errors::SecretsError;
pub use path::SecretPath;
pub use values::{SecretMetadata, SecretValues};

/// One client's secrets, addressed within that client's boundary.
#[async_trait]
pub trait ClientSecrets: Send + Sync {
    /// Every secret path inside the boundary.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError`] if the boundary could not be read.
    async fn list(&self, namespace: &SecretNamespace) -> Result<Vec<SecretPath>, SecretsError>;

    /// What is known about one secret **without revealing it**.
    ///
    /// Key names, versions and timestamps. A console lists secrets constantly
    /// and reveals one rarely, so the cheap operation is the one that cannot
    /// leak.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError`] if the secret is absent or could not be read.
    async fn metadata(
        &self,
        namespace: &SecretNamespace,
        path: &SecretPath,
    ) -> Result<SecretMetadata, SecretsError>;

    /// The current values, fetched because somebody deliberately asked.
    ///
    /// Separate from [`Self::metadata`] so that revealing is an action in the
    /// audit trail rather than a side effect of looking at a list.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError`] if the secret is absent or could not be read.
    async fn reveal(
        &self,
        namespace: &SecretNamespace,
        path: &SecretPath,
    ) -> Result<SecretValues, SecretsError>;

    /// Writes a secret, refusing a write against a version somebody has moved
    /// past.
    ///
    /// `expected` is the version the operator was looking at. `None` means
    /// they believe the secret does not exist yet; a mismatch is
    /// [`SecretsError::Conflict`] rather than an overwrite.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError`] on a version conflict or if the write failed.
    async fn write(
        &self,
        namespace: &SecretNamespace,
        path: &SecretPath,
        values: &SecretValues,
        expected: Option<u64>,
    ) -> Result<u64, SecretsError>;

    /// Removes a secret and every version of it.
    ///
    /// # Errors
    ///
    /// Returns [`SecretsError`] if the secret is absent or could not be
    /// removed.
    async fn delete(&self, namespace: &SecretNamespace, path: &SecretPath) -> Result<(), SecretsError>;
}
