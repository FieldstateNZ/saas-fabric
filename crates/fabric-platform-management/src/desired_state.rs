//! The port through which an environment's desired state is read and moved.

use std::collections::BTreeMap;

use crate::{Channel, ReleaseUnit, UpdatePolicy, Version};

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

/// What can go wrong reading or moving desired state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DesiredStateError {
    /// No such environment, or no such component in it.
    #[error("{what} is not something this platform describes")]
    NotFound {
        /// What was asked for.
        what: String,
    },

    /// Something the write was editing changed since it was read.
    ///
    /// Not a failure so much as an instruction: the decision was taken against
    /// state that has moved, so it has to be taken again.
    #[error("desired state changed while it was being written")]
    Conflict,

    /// The store could not be reached, or failed internally.
    #[error("desired state is unavailable: {detail}")]
    Unavailable {
        /// What was observed, with no credential in it.
        detail: String,
    },

    /// The store understood the request and refused it.
    #[error("desired state refused the change: {detail}")]
    Refused {
        /// What was observed, with no credential in it.
        detail: String,
    },
}

/// Where an environment's desired state is kept.
///
/// Implemented by an adapter that knows how the platform repository is laid
/// out. Nothing here knows which files carry a pin — that is the platform
/// repository's own statement, and asking this port to move a component is the
/// whole of what this crate does about it.
#[async_trait::async_trait]
pub trait DesiredState: Send + Sync {
    /// Every component an environment describes.
    ///
    /// Read rather than configured, so adding a component to the platform
    /// repository is enough to have it reconciled — a second list in Fabric's
    /// configuration would be a second thing to keep in step, and the failure
    /// when it drifted would be a component nothing was looking after.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError`] if the environment cannot be read.
    async fn components(&self, environment: &str) -> Result<Vec<String>, DesiredStateError>;

    /// What an environment is asked to run of a component.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError`] if it cannot be read.
    async fn component(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<ComponentDesired, DesiredStateError>;

    /// Moves a component onto a release unit.
    ///
    /// Takes the unit discovery assembled, so the version, the source commit
    /// and every image's digest travel together. There is no way to express
    /// moving one image, changing a policy, or clearing a hold.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError::Conflict`] if the state moved since it was read,
    /// and the other variants otherwise.
    async fn advance(
        &self,
        environment: &str,
        component: &str,
        unit: &ReleaseUnit,
        message: &str,
    ) -> Result<(), DesiredStateError>;
}
