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
}
