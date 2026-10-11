//! The two entries a lifecycle record can hold.

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
