//! What desired state records about a client's lifecycle (`spec.lifecycle`),
//! and the two operator transitions that write it.
//!
//! In the 121-150 band: the record and the two transitions that are the only
//! way to produce a new one. Splitting a transition from the record it moves
//! would put the rule "deletion follows a tombstone" in a different file from
//! the field that holds the tombstone.

use serde::{Deserialize, Serialize};

use super::entries::{DeletionConfirmation, Tombstone};
use super::refusal::LifecycleRefusal;
use crate::ClientId;

/// The longest tombstone reason accepted, in bytes.
const MAX_REASON: usize = 512;

/// A client's recorded lifecycle (D01-14).
///
/// Absent from a document means neither entry is present: the client is
/// saved intent, and whether it is usable is derived from evidence
/// ([`super::ClientLifecycle::derive`]), never recorded here.
///
/// `deny_unknown_fields`, unlike the rest of `spec`: a misspelt `tombstoned:`
/// would otherwise read as an active client, which is the one misreading this
/// section exists to rule out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifecycleRecord {
    /// When and why the client was taken out of service, retaining its state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tombstone: Option<Tombstone>,

    /// When deletion of a tombstoned client was confirmed. Records intent
    /// only: nothing is deleted when this is written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletion_confirmed: Option<DeletionConfirmation>,
}

impl LifecycleRecord {
    /// Whether the client has been taken out of service.
    #[must_use]
    pub const fn is_retired(&self) -> bool {
        self.tombstone.is_some()
    }

    /// Tombstones an active client.
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
        let reason = check_reason(reason)?;

        Ok(Self {
            tombstone: Some(Tombstone {
                at,
                by: by.to_owned(),
                reason,
            }),
            deletion_confirmed: None,
        })
    }

    /// Confirms deletion of a tombstoned client, naming it again.
    ///
    /// # Errors
    ///
    /// [`LifecycleRefusal::ConfirmationMismatch`] when `confirming` is not
    /// `client`, [`LifecycleRefusal::DeletionRequiresTombstone`] for a client
    /// that was never tombstoned, and
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
        let Some(tombstone) = &self.tombstone else {
            return Err(LifecycleRefusal::DeletionRequiresTombstone);
        };

        Ok(Self {
            tombstone: Some(tombstone.clone()),
            deletion_confirmed: Some(DeletionConfirmation {
                at,
                by: by.to_owned(),
            }),
        })
    }

    /// The invariants a stored record must keep, checked when a document is
    /// read so a hand edit cannot confirm deletion without a tombstone.
    pub(crate) fn check(&self) -> Result<(), &'static str> {
        match (&self.tombstone, &self.deletion_confirmed) {
            (None, Some(_)) => Err("deletionConfirmed requires a tombstone"),
            (Some(tombstone), Some(deletion)) if deletion.at < tombstone.at => {
                Err("deletionConfirmed must not precede the tombstone")
            }
            (Some(tombstone), _) => check_reason(&tombstone.reason)
                .map(|_| ())
                .map_err(|_| "tombstone.reason is invalid"),
            (None, None) => Ok(()),
        }
    }
}

fn check_reason(reason: &str) -> Result<String, LifecycleRefusal> {
    let trimmed = reason.trim();
    let detail = if trimmed.is_empty() {
        "a tombstone needs a reason"
    } else if trimmed.len() > MAX_REASON {
        "a tombstone reason must be at most 512 bytes"
    } else if trimmed.chars().any(char::is_control) {
        "a tombstone reason must not contain control characters"
    } else {
        return Ok(trimmed.to_owned());
    };

    Err(LifecycleRefusal::InvalidReason { detail })
}
