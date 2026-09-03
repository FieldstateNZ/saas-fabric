//! What an environment is asked to run of one component.

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

/// What desired state looked like when it was read.
///
/// # Why a decision carries this
///
/// Everything a selector decides is decided against a *read*: the version to
/// move from, the policy that permits it, the hold that would stop it. If the
/// write then re-reads and applies the decision to whatever it finds, an
/// operator who added a hold in between watches it be ignored — the decision
/// was correct when it was taken and wrong by the time it landed.
///
/// So the read hands back a revision, the write demands it, and desired state
/// that moved in between is a [`Conflict`](crate::DesiredStateError::Conflict)
/// rather than a silent overwrite. Opaque on purpose: a caller compares it and
/// hands it back, and never parses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesiredRevision(String);

impl DesiredRevision {
    /// Records what an adapter read.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// The token, for an adapter comparing it with its own.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
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

    /// Where this component's versions are published, and how.
    pub source: crate::ArtifactSource,

    /// What desired state was when this was read.
    ///
    /// Every write built from this read must present it, so a decision taken
    /// against state that has since moved is refused rather than applied.
    pub revision: DesiredRevision,
}
