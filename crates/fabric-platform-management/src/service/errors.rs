//! What can go wrong looking at, or moving, a component.

use crate::{DesiredStateError, RegistryError};

/// What can go wrong looking at, or moving, a component.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlatformError {
    /// Desired state could not be read or written.
    #[error(transparent)]
    DesiredState(#[from] DesiredStateError),

    /// A registry could not be asked.
    ///
    /// Kept distinct because it is the failure that changes nothing: desired
    /// state is untouched and availability is merely stale.
    #[error(transparent)]
    Registry(#[from] RegistryError),

    /// The component is not one that advances, so pausing it means nothing.
    ///
    /// Separate from a transport failure because the request was understood
    /// and the state does not permit it — and a component that is `Manual` or
    /// `Locked` already does not advance. Recording a hold on one would put a
    /// pause in the manifest that stops nothing, and show an operator
    /// "Paused" about a component that was never moving.
    #[error("{component} does not advance on its own, so there is nothing to pause")]
    NotAdvancing {
        /// Which component was asked.
        component: String,
    },

    /// The version asked for is not one this component can be rolled back to.
    ///
    /// It does not exist below the desired one; or, for images, its images are
    /// incomplete or disagree about their source commit, so it is not a
    /// release unit anything ever ran; or, for a chart, the repository no
    /// longer lists it.
    ///
    /// # Not "this kind of component cannot be rolled back"
    ///
    /// There is no such refusal. Rolling back restores a previously selected
    /// desired version and is offered for both artifact kinds — what differs
    /// is how much of the old release comes back, and that is said to the
    /// operator rather than enforced by declining. This variant is about the
    /// one version they named.
    #[error("{version} is not a version {component} can be rolled back to")]
    NotRollable {
        /// Which component was asked.
        component: String,

        /// What was asked for, as the caller wrote it.
        version: String,
    },
}
