//! What an environment is asked to run of one component.

use std::collections::BTreeMap;

use crate::{Channel, UpdatePolicy, Version};

/// Why an operator's hold is in place.
///
/// Carries no version. The desired version already *is* the held one, so a
/// break-glass edit that moves it by hand leaves the hold correctly in force
/// rather than pointing at something nothing runs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hold {
    /// What the operator gave as the reason.
    pub reason: String,

    /// When advancement stopped, as an RFC 3339 timestamp.
    pub since: String,

    /// What they wanted the next person to know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// What an environment is asked to run of one component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentDesired {
    /// The version desired now.
    pub version: Version,

    /// The release stream newer versions are drawn from.
    pub channel: Channel,

    /// The standing decision about advancement.
    pub policy: UpdatePolicy,

    /// Present while advancement is paused.
    pub hold: Option<Hold>,

    /// Where each of the component's images is published, by role.
    pub repositories: BTreeMap<String, String>,
}
