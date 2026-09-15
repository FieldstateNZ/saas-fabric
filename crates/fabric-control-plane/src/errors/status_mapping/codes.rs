//! The stable code a client branches on, beside each status.

use fabric_platform_management::{DesiredStateError, PlatformError};

use crate::ControlPlaneError;

impl ControlPlaneError {
    /// A stable machine-readable code, so a client branches on this rather
    /// than on message text.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Unauthenticated(_) => "unauthenticated",
            Self::UnknownClient(_) => "unknown_client",
            Self::InvalidRequest(_) => "invalid_request",
            // The catalogue shares this code with a client's document
            // deliberately — see `ControlPlaneError::InvalidCatalogue` — so a
            // console does not need a second code to know what to do with
            // it.
            Self::InvalidDesiredState { .. } | Self::InvalidCatalogue { .. } => "desired_state_invalid",
            Self::RevisionRequired => "revision_required",
            // The catalogue shares this code with a client's revision
            // conflict deliberately — see `ControlPlaneError::CatalogueRevisionConflict`
            // — the same remedy either way: read again, redo the edit.
            Self::RevisionConflict | Self::CatalogueRevisionConflict => "revision_conflict",
            // Its own code beside `revision_conflict`: the two share a status
            // but not a remedy. A stale write is fixed by re-reading and
            // redoing the edit; a taken id is fixed by choosing a different
            // one, and a console that could only see `409` could not tell
            // which screen to show.
            Self::ClientExists { .. } => "client_exists",
            // Its own code too, beside `client_exists` and `realm_immutable`:
            // three different reasons a name cannot be used, and three
            // different next steps — pick a different id, pick a different
            // realm, or accept the realm is fixed.
            Self::RealmUnavailable { .. } => "realm_unavailable",
            Self::DocumentTooLarge { .. } => "document_too_large",
            Self::RealmImmutable { .. } => "realm_immutable",
            Self::RepositoryUnavailable => "repository_unavailable",
            Self::InvalidFlow => "invalid_flow",
            Self::ConvergenceUnavailable => "convergence_unavailable",
            Self::IntegrationNotManaged => "integration_not_managed",
            // One code for two situations, deliberately. "This deployment
            // does no platform management" and "an operator has not connected
            // a repository" are different things to the platform and the same
            // thing to a console: there is nothing here yet. What is *not*
            // folded in is a connected repository that fails, which is below.
            Self::PlatformNotManaged
            | Self::Platform(PlatformError::DesiredState(DesiredStateError::NotConnected)) => {
                "platform_not_managed"
            }
            Self::Platform(PlatformError::DesiredState(DesiredStateError::NotFound { .. })) => {
                "component_unknown"
            }
            Self::Platform(PlatformError::NotAdvancing { .. }) => "component_not_advancing",
            // Its own code beside `revision_conflict`, because they are not the
            // same event to a console: that one is a client's desired state
            // moving, this one is a platform component's. Both mean "read again
            // and redo it", and a console that could only see `409` would not
            // know which page to reload.
            Self::Platform(PlatformError::DesiredState(DesiredStateError::Conflict)) => {
                "platform_state_moved"
            }
            Self::Platform(PlatformError::NotRollable { .. }) => "version_not_rollable",
            Self::Platform(_) => "platform_unavailable",
            Self::GitHostRefused => "git_host_refused",
            Self::IntegrationRefused(_) => "integration_refused",
            // Its own code beside `revision_conflict` and `platform_state_moved`,
            // for the reason those two are apart: all three mean "read again and
            // redo it", and a caller that could only see `409` would not know
            // whether it was a client, a component, or the integration page that
            // moved. Today that caller is a log reader or an API client; the
            // console shows the message and does not yet reload on it.
            Self::IntegrationMoved => "integration_moved",
            Self::IntegrationNotConfigured => "integration_not_configured",

            // Distinct codes for statuses that collide. A console that could
            // only see `409` would have to guess whether to offer "reload" or
            // "this client has no secret boundary yet".
            Self::Secrets(secrets) => match secrets {
                crate::SecretsError::NoBoundary => "secret_no_boundary",
                crate::SecretsError::NotFound => "secret_not_found",
                crate::SecretsError::Conflict => "secret_stale_version",
                crate::SecretsError::Refused => "secret_store_refused",
                crate::SecretsError::Unavailable => "secret_store_unavailable",
            },
            Self::SignInRefused => "sign_in_refused",
            Self::SignInUnavailable => "sign_in_unavailable",
            Self::RepositoryDenied => "repository_denied",
            Self::RepositoryRejected => "repository_rejected",
        }
    }
}
