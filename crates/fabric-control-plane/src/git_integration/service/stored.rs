//! Reading back what this platform recorded about its integration.
//!
//! Two stores, and the pair is the point: the record says which application
//! and repository, the key is what lets the platform act as it. Every step
//! past creation needs both, and either one alone describes an integration
//! that cannot do anything.

use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::git_integration::{SecretName, SecretValue};
use crate::GitIntegration;

/// What a transition was prepared against, read in the one order that is safe.
///
/// Carried as a value rather than handed back as a tuple, so the generation
/// cannot be dropped on the floor between reading it and passing it on — a
/// transition that lost it would be one nothing could refuse.
pub(super) struct Prepared {
    /// The generation the record and the key were read under.
    pub(super) generation: u64,

    /// The integration as it stood then.
    pub(super) integration: GitIntegration,

    /// The application's private key as it stood then.
    pub(super) key: SecretValue,
}

impl GitIntegrationService {
    /// The generation, the record and the key, in that order.
    ///
    /// **The order is the whole point.** A generation read *after* the record
    /// would miss a transition that landed between the two reads: the record
    /// would be the old one and the generation the new one, so the compare
    /// when this transition takes its turn would find them equal and let it
    /// through carrying state nobody checked. Read first, the generation is at
    /// worst too old, and too old is refused.
    ///
    /// Both callers that settle a repository come through here, so neither can
    /// get that order wrong — which is the sort of mistake that surfaces as a
    /// resurrected integration months later rather than as a failing test.
    ///
    /// # Errors
    ///
    /// [`IntegrationError::NotConnected`] when there is no record or no key —
    /// the state of a platform that has never connected, and of one an
    /// operator has just disconnected — or [`IntegrationError::Unavailable`]
    /// when a store could not be read.
    pub(super) async fn prepared(&self) -> Result<Prepared, IntegrationError> {
        let generation = self.transitions.observed();
        let integration = self.current().await?.ok_or(IntegrationError::NotConnected)?;
        let key = self.private_key().await?;

        Ok(Prepared {
            generation,
            integration,
            key,
        })
    }

    /// The stored integration, if this platform has one.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError`] if the store could not be read.
    pub async fn current(&self) -> Result<Option<GitIntegration>, IntegrationError> {
        self.store.load(self.kind).await.map_err(IntegrationError::from)
    }

    /// The application's private key.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::NotConnected`] when there is no key, which
    /// is the state of a platform that has never connected — and
    /// [`IntegrationError::Unavailable`] when the store could not be read.
    pub(super) async fn private_key(&self) -> Result<SecretValue, IntegrationError> {
        self.secrets
            .get(&SecretName::new(self.kind.private_key()))
            .await?
            .ok_or(IntegrationError::NotConnected)
    }
}
