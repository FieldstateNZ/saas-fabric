//! What can go wrong looking at, or moving, a component.

use crate::{DataSourceRule, DesiredStateError, RegistryError};

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
    /// There is no such refusal. Rolling back restores an older published
    /// version and is offered for both artifact kinds — what differs
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

    /// A declared data source breaks one of ADR 0023 part 1's placement
    /// rules.
    ///
    /// Kept distinct from `DesiredState` above because the request was
    /// understood and refused on its own terms, before anything was read
    /// or written -- the same reason `NotAdvancing` is not folded into a
    /// transport failure either.
    #[error(transparent)]
    InvalidDataSource(#[from] DataSourceRule),

    /// A held data-sources document is no longer something to trust: a
    /// hand edit gave two entries the same id, or made an entry fail its
    /// own validation (`data_sources::held::check_held`, run on every
    /// read).
    ///
    /// Kept apart from `DesiredState`'s own `Refused` above, which an
    /// adapter also answers for its own reasons -- a revoked credential,
    /// a host rejecting a write, a document declaring a schema version
    /// this platform does not read. Those are outage-shaped failures
    /// upstream of this platform and get the generic mapping's retryable
    /// answer; this one is a coherence problem in a document this
    /// platform itself authored, and no retry fixes it. Collapsing the
    /// two would tell an operator whose GitHub App was revoked that their
    /// data-sources file is broken.
    #[error("{detail}")]
    InvalidHeldDataSources {
        /// What `check_held` found wrong, in its own words -- never a
        /// file path.
        detail: String,
    },
}
