//! Why a connection step could not be completed.

use crate::git_integration::{IntegrationStoreError, ProvisioningError, SecretStoreError};
use crate::ControlPlaneError;

/// A failure somewhere in the connection flow.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IntegrationError {
    /// The callback did not name a flow this platform started.
    ///
    /// A token that was never issued, has already been spent, has expired, or
    /// belongs to the other leg. All four are the same thing to an operator —
    /// start again — and telling them apart in the response would tell an
    /// attacker which of their guesses was closest.
    #[error("that connection did not start here; start it again")]
    NotOurFlow,

    /// The platform holds no integration to act on.
    #[error("this platform is not connected to a client desired-state repository yet")]
    NotConnected,

    /// The Git host refused something.
    #[error("the Git host refused the request")]
    HostRefused,

    /// A dependency of the flow could not be reached.
    #[error("a service this connection needs is unavailable")]
    Unavailable,

    /// The operator asked for something the platform cannot do.
    #[error("{0}")]
    Refused(String),
}

impl From<ProvisioningError> for IntegrationError {
    fn from(error: ProvisioningError) -> Self {
        match error {
            ProvisioningError::Refused => Self::HostRefused,
            ProvisioningError::Unavailable => Self::Unavailable,
        }
    }
}

impl From<SecretStoreError> for IntegrationError {
    /// Every secret-store failure becomes unavailable, deliberately.
    ///
    /// A refused credential, an unreachable store and a read-only one are
    /// different problems for whoever runs the platform and the same problem
    /// for the operator in front of the console: the connection cannot be
    /// completed and no amount of clicking fixes it. The distinction is in the
    /// log, where the person who can act on it looks.
    fn from(_: SecretStoreError) -> Self {
        Self::Unavailable
    }
}

impl From<IntegrationStoreError> for IntegrationError {
    fn from(_: IntegrationStoreError) -> Self {
        Self::Unavailable
    }
}

impl From<IntegrationError> for ControlPlaneError {
    fn from(error: IntegrationError) -> Self {
        match error {
            IntegrationError::NotOurFlow => Self::InvalidFlow,
            IntegrationError::NotConnected => Self::IntegrationNotConfigured,
            IntegrationError::HostRefused => Self::GitHostRefused,
            IntegrationError::Unavailable => Self::RepositoryUnavailable,
            IntegrationError::Refused(detail) => Self::IntegrationRefused(detail),
        }
    }
}
