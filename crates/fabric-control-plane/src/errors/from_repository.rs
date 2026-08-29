//! Translating a repository failure into the operator-facing error.

use crate::repository::RepositoryError;
use crate::ControlPlaneError;

impl ControlPlaneError {
    /// Translates a repository failure into the operator-facing error.
    ///
    /// Deliberately not a `From` impl. Every call site has to name the client
    /// it was working on, because [`RepositoryError::Unavailable`] carries a
    /// detail that must not reach the browser and this is where it is dropped
    /// — a blanket conversion would make it easy to add a call site that
    /// forwards it.
    #[must_use]
    pub(crate) fn from_repository(error: RepositoryError) -> Self {
        match error {
            RepositoryError::NotConfigured => Self::IntegrationNotConfigured,
            RepositoryError::NotFound { client } => Self::UnknownClient(client),
            RepositoryError::Conflict => Self::RevisionConflict,
            RepositoryError::Unavailable { .. } => Self::RepositoryUnavailable,
            RepositoryError::NotPermitted => Self::RepositoryDenied,
            RepositoryError::Rejected { .. } => Self::RepositoryRejected,
            RepositoryError::Invalid { client, source } => Self::InvalidDesiredState { client, source },
        }
    }
}
