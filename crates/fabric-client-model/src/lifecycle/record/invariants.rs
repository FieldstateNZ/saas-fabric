//! The invariants a stored lifecycle record must keep, checked when a
//! document is read so a hand edit cannot confirm deletion without a
//! tombstone, or write a history that runs backwards.

use super::LifecycleRecord;

/// The longest tombstone or restoration reason accepted, in bytes.
const MAX_REASON: usize = 512;

/// Which entry a reason belongs to, for the words a refusal uses.
#[derive(Clone, Copy)]
pub(super) enum Reasoned {
    Tombstone,
    Restoration,
}

impl LifecycleRecord {
    /// Every rule a record read from a document must keep.
    ///
    /// Restorations are in order: each reverses a tombstone written before
    /// it, and each tombstone after the first follows the restoration before
    /// it. A current tombstone follows the last restoration. This does not
    /// prove that a restoration was an operator's write rather than a Git
    /// edit; only the protected continuity store could (#108 comment
    /// 6119161317).
    pub(crate) fn check(&self) -> Result<(), &'static str> {
        match (&self.tombstone, &self.deletion_confirmed) {
            (None, Some(_)) => return Err("deletionConfirmed requires a tombstone"),
            (Some(tombstone), Some(deletion)) if deletion.at < tombstone.at => {
                return Err("deletionConfirmed must not precede the tombstone")
            }
            _ => {}
        }
        if let Some(tombstone) = &self.tombstone {
            check_reason(Reasoned::Tombstone, &tombstone.reason)
                .map_err(|_| "tombstone.reason is invalid")?;
        }

        let mut floor = 0;
        for restoration in &self.restorations {
            check_reason(Reasoned::Tombstone, &restoration.tombstone.reason)
                .map_err(|_| "restorations[].tombstone.reason is invalid")?;
            check_reason(Reasoned::Restoration, &restoration.reason)
                .map_err(|_| "restorations[].reason is invalid")?;
            if restoration.tombstone.at < floor || restoration.at < restoration.tombstone.at {
                return Err("restorations must each follow the tombstone they reverse, in order");
            }
            floor = restoration.at;
        }
        match &self.tombstone {
            Some(tombstone) if tombstone.at < floor => {
                Err("the tombstone must not precede the last restoration")
            }
            _ => Ok(()),
        }
    }
}

/// The trimmed reason, or what is wrong with it.
pub(super) fn check_reason(entry: Reasoned, reason: &str) -> Result<String, &'static str> {
    let trimmed = reason.trim();
    let detail = match (entry, trimmed) {
        (Reasoned::Tombstone, "") => "a tombstone needs a reason",
        (Reasoned::Restoration, "") => "a restoration needs a reason",
        (Reasoned::Tombstone, _) if trimmed.len() > MAX_REASON => {
            "a tombstone reason must be at most 512 bytes"
        }
        (Reasoned::Restoration, _) if trimmed.len() > MAX_REASON => {
            "a restoration reason must be at most 512 bytes"
        }
        (Reasoned::Tombstone, _) if trimmed.chars().any(char::is_control) => {
            "a tombstone reason must not contain control characters"
        }
        (Reasoned::Restoration, _) if trimmed.chars().any(char::is_control) => {
            "a restoration reason must not contain control characters"
        }
        _ => return Ok(trimmed.to_owned()),
    };

    Err(detail)
}
