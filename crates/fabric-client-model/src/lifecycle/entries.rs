//! The entries a lifecycle record can hold.

use serde::{Deserialize, Serialize};

/// The tombstone entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tombstone {
    /// When it was written, in seconds since the Unix epoch.
    pub at: u64,
    /// The operator subject that wrote it.
    pub by: String,
    /// Why, in the operator's words.
    pub reason: String,
}

/// The deletion-confirmed entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeletionConfirmation {
    /// When it was written, in seconds since the Unix epoch.
    pub at: u64,
    /// The operator subject that wrote it.
    pub by: String,
}

/// One tombstone an operator reversed, kept with the reversal so restoring a
/// client never erases that it was retired (#108 comment 6119161317, item 2:
/// a reinstatement is a new forward transition, not a reset).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Restoration {
    /// The tombstone that was reversed, exactly as it was recorded.
    pub tombstone: Tombstone,
    /// When it was reversed, in seconds since the Unix epoch.
    pub at: u64,
    /// The operator subject that reversed it.
    pub by: String,
    /// Why, in the operator's words.
    pub reason: String,
}
