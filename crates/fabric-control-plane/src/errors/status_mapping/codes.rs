//! The stable code a client branches on, beside each status.

use fabric_platform_management::{DesiredStateError, PlatformError};

use crate::ControlPlaneError;

impl ControlPlaneError {
    /// A stable machine-readable code, so a client branches on this rather
    /// than on message text.
    ///
    /// `InvalidDataSource`'s two arms -- the direct variant and the one
    /// structurally wrapped in `Platform` -- share a body because they are
    /// the same failure reaching this match two different ways, not two
    /// causes that happen to agree.
    #[allow(
        clippy::match_same_arms,
        reason = "InvalidDataSource's two arms are one cause reaching this match two ways"
    )]
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Unauthenticated(_) => "unauthenticated",
            // Its own code beside `unauthenticated`: a console (ADR 0024's
            // gateway probe, first) needs to tell "nobody is signed in" apart
            // from "somebody was, and the bearer was refused" on the very
            // first response it ever sees.
            Self::OperatorRefused => "operator_refused",
            Self::UnknownClient(_) => "unknown_client",
            Self::InvalidRequest(_) => "invalid_request",
            // The catalogue shares this code with a client's document
            // deliberately — see `ControlPlaneError::InvalidCatalogue` — so a
            // console does not need a second code to know what to do with
            // it. A held data-sources document that fails the same way
            // shares the code too, from the structural arm below.
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
            // Structural, so a declaration's own rule violation keeps its
            // code (422, `invalid_data_source`) whether a handler unwraps it
            // itself or lets `?` wrap it in `Self::Platform` -- see
            // `Self::InvalidDataSource` above, which this shares a status and
            // code with.
            Self::Platform(PlatformError::InvalidDataSource(_)) => "invalid_data_source",
            // A held data-sources document a hand edit made incoherent --
            // a duplicate id, or an entry that no longer validates.
            // Shares `InvalidDesiredState`/`InvalidCatalogue`'s code above
            // for the reason their own doc comments give: one thing to do
            // with any of the three, stop and do not retry.
            Self::Platform(PlatformError::InvalidHeldDataSources { .. }) => "desired_state_invalid",
            // ADR 0023 part 2's placement refusal, whichever of `select`'s
            // rules stopped it. Shares its code with
            // `LogicalDataSourceNotDeclared` above -- see that variant's
            // rustdoc for why the two share one answer.
            Self::Platform(PlatformError::PlacementRefused(_)) => "placement_refused",
            // A held placements document a hand edit made incoherent.
            // Shares `InvalidHeldDataSources`'s code above for the same
            // reason that one shares `InvalidDesiredState`/`InvalidCatalogue`'s.
            Self::Platform(PlatformError::InvalidHeldPlacements { .. }) => "desired_state_invalid",
            // Its own code: a data source cannot be removed while a
            // placement still names it, and the fix -- unplace every
            // tenant first -- is different from any other `409` this API
            // answers.
            Self::Platform(PlatformError::DataSourceInUse { .. }) => "data_source_in_use",
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
            Self::InvalidDataSource(_) => "invalid_data_source",
            // Shares `PlacementRefused`'s code above -- see
            // `ControlPlaneError::LogicalDataSourceNotDeclared`'s rustdoc.
            Self::LogicalDataSourceNotDeclared { .. } => "placement_refused",
            Self::PublicationNotConfigured => "publication_not_configured",
            Self::PublicationRunning => "publication_running",
        }
    }
}
