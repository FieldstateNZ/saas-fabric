//! What desired state records about a client's lifecycle (`spec.lifecycle`),
//! and the three operator transitions that write it.
//!
//! In the 121-150 band: the record and the only three transitions producing a
//! new one, kept with the fields they move. Invariants: `invariants`.

mod invariants;

use serde::{Deserialize, Serialize};

use super::entries::{DeletionConfirmation, Restoration, Tombstone};
use super::instance::IdentityInstance;
use super::refusal::LifecycleRefusal;
use crate::ClientId;
use invariants::{check_reason, Reasoned};

/// A client's recorded lifecycle (D01-14).
///
/// Absent means saved intent; whether it is usable is derived from evidence
/// ([`super::ClientLifecycle::derive`]), never recorded here.
///
/// `deny_unknown_fields`, unlike the rest of `spec`: a misspelt `tombstoned:`
/// would otherwise read as an active client.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifecycleRecord {
    /// When and why the client was taken out of service, retaining its state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tombstone: Option<Tombstone>,

    /// When deletion of a tombstoned client was confirmed. Intent only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletion_confirmed: Option<DeletionConfirmation>,

    /// Every tombstone an operator reversed, oldest first. Only ever grows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restorations: Vec<Restoration>,

    /// The identity instance the continuity store attested at creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<IdentityInstance>,
}

impl LifecycleRecord {
    /// Whether the client has been taken out of service.
    #[must_use]
    pub const fn is_retired(&self) -> bool {
        self.tombstone.is_some()
    }

    /// Tombstones an active client, keeping every earlier restoration.
    ///
    /// # Errors
    ///
    /// [`LifecycleRefusal::DeletionAlreadyConfirmed`] or
    /// [`LifecycleRefusal::AlreadyTombstoned`] for a retired client, and
    /// [`LifecycleRefusal::InvalidReason`] for a blank, oversized or
    /// control-character reason.
    pub fn tombstone(&self, at: u64, by: &str, reason: &str) -> Result<Self, LifecycleRefusal> {
        if self.deletion_confirmed.is_some() {
            return Err(LifecycleRefusal::DeletionAlreadyConfirmed);
        }
        if self.tombstone.is_some() {
            return Err(LifecycleRefusal::AlreadyTombstoned);
        }
        let reason = check_reason(Reasoned::Tombstone, reason)
            .map_err(|detail| LifecycleRefusal::InvalidReason { detail })?;

        Ok(Self {
            tombstone: Some(Tombstone {
                at,
                by: by.to_owned(),
                reason,
            }),
            deletion_confirmed: None,
            ..self.clone()
        })
    }

    /// Confirms deletion of a tombstoned client, naming it again.
    ///
    /// # Errors
    ///
    /// [`LifecycleRefusal::ConfirmationMismatch`] when `confirming` is not
    /// `client`, [`LifecycleRefusal::DeletionRequiresTombstone`] for a client
    /// that is not tombstoned, and
    /// [`LifecycleRefusal::DeletionAlreadyConfirmed`] for a second
    /// confirmation.
    pub fn confirm_deletion(
        &self,
        client: &ClientId,
        confirming: &ClientId,
        at: u64,
        by: &str,
    ) -> Result<Self, LifecycleRefusal> {
        if client != confirming {
            return Err(LifecycleRefusal::ConfirmationMismatch);
        }
        if self.deletion_confirmed.is_some() {
            return Err(LifecycleRefusal::DeletionAlreadyConfirmed);
        }
        if self.tombstone.is_none() {
            return Err(LifecycleRefusal::DeletionRequiresTombstone);
        }

        Ok(Self {
            deletion_confirmed: Some(DeletionConfirmation {
                at,
                by: by.to_owned(),
            }),
            ..self.clone()
        })
    }

    /// Reverses the current tombstone, moving it into
    /// [`restorations`](Self::restorations) with who reversed it and why.
    ///
    /// # Errors
    ///
    /// [`LifecycleRefusal::DeletionConfirmedNotRestorable`] once deletion is
    /// confirmed, [`LifecycleRefusal::RestoreRequiresTombstone`] for a client
    /// that is not tombstoned, and
    /// [`LifecycleRefusal::InvalidRestorationReason`] for a blank, oversized
    /// or control-character reason.
    pub fn restore(&self, at: u64, by: &str, reason: &str) -> Result<Self, LifecycleRefusal> {
        if self.deletion_confirmed.is_some() {
            return Err(LifecycleRefusal::DeletionConfirmedNotRestorable);
        }
        let Some(tombstone) = &self.tombstone else {
            return Err(LifecycleRefusal::RestoreRequiresTombstone);
        };
        let reason = check_reason(Reasoned::Restoration, reason)
            .map_err(|detail| LifecycleRefusal::InvalidRestorationReason { detail })?;

        let mut restorations = self.restorations.clone();
        restorations.push(Restoration {
            tombstone: tombstone.clone(),
            at,
            by: by.to_owned(),
            reason,
        });

        Ok(Self {
            tombstone: None,
            deletion_confirmed: None,
            restorations,
            instance: self.instance.clone(),
        })
    }
}
