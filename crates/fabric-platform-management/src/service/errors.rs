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
    /// It does not exist below the desired one, or its images are incomplete
    /// or disagree about their source commit. Either way it is not a release
    /// unit anything ever ran, and deploying it would be deploying a
    /// composition that never existed.
    /// This component is published as something rollback cannot promise about.
    ///
    /// # Not "not implemented yet"
    ///
    /// Rolling back an image pins a digest: the bytes an environment goes back
    /// to are the bytes it ran. A classic chart repository pins a *version*,
    /// and the bytes behind `7.3.0` can be republished — so "put me back on
    /// what I was running" is a promise it cannot keep, and offering the
    /// control anyway would be offering a guarantee this platform does not
    /// have.
    ///
    /// It becomes possible when chart lifecycle is modelled — an OCI chart
    /// registry, or a digest recorded at the moment of deployment. Until then
    /// the honest answer is that this component cannot be rolled back, and an
    /// operator's route back is a deliberate forward change to the version
    /// they want.
    #[error("{component} is published as {artifact}, which cannot be rolled back")]
    RollbackUnsupported {
        /// Which component was asked.
        component: String,

        /// What it is published as, in an operator's words.
        artifact: &'static str,
    },

    /// The version asked for is not one this component can be rolled back to.
    ///
    /// It does not exist below the desired one, or its images are incomplete
    /// or disagree about their source commit. Either way it is not a release
    /// unit anything ever ran.
    #[error("{version} is not a version {component} can be rolled back to")]
    NotRollable {
        /// Which component was asked.
        component: String,

        /// What was asked for, as the caller wrote it.
        version: String,
    },
}
