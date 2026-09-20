//! Which status code, and which stable machine code, each failure carries.

use http::StatusCode;

mod codes;
mod messages;
mod platform;

use crate::ControlPlaneError;

impl ControlPlaneError {
    /// The status code the operator's browser sees.
    ///
    /// # Why arms repeat a status
    ///
    /// This match is organised by *cause*, not by code, and several causes
    /// honestly share a status: four different absences are 404, and two
    /// unrelated conflicts are 409. Merging them would put unrelated failures
    /// in one arm and delete the comment explaining each — and the distinction
    /// an operator acts on survives anyway in the machine code beside it, where
    /// every one of them has its own. The platform's own arms follow the same
    /// rule in `platform.rs`.
    #[allow(
        clippy::match_same_arms,
        reason = "arms are grouped by cause; codes keep them apart"
    )]
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            // A refused redemption joins an unauthenticated request at 401,
            // and for the same reason: in both cases the operator holds no
            // usable identity and their next step is to sign in.
            Self::Unauthenticated(_) | Self::SignInRefused => StatusCode::UNAUTHORIZED,
            // 404: for this deployment, none of these exists -- a client,
            // the connection surface, or the publication target. Not a 403
            // (no grant would fix an absence) and not a 503 (no wait would
            // either): only a config edit and a restart change any of them.
            Self::UnknownClient(_)
            | Self::IntegrationNotManaged
            | Self::PlatformNotManaged
            | Self::ConvergenceUnavailable
            | Self::PublicationNotConfigured => StatusCode::NOT_FOUND,
            // `InvalidFlow` joins these at 400 rather than 401, and that is
            // the interesting one: its caller is a browser the Git host
            // redirected here, holding no identity to be wrong about. What is
            // wrong is the callback, and answering 401 would send an operator
            // round a sign-in loop that cannot end.
            Self::InvalidRequest(_)
            | Self::RealmImmutable { .. }
            | Self::IntegrationRefused(_)
            | Self::InvalidFlow => StatusCode::BAD_REQUEST,

            // Platform Management has six of these, spread over four statuses
            // and each with its own reason. They live next door rather than
            // swamping this match.
            Self::Platform(platform) => platform::status(platform),

            // 428, not 400. The request is well-formed; what is missing is the
            // precondition that makes it safe to apply, and 428 is the status
            // that says exactly that. A client seeing it knows to read the
            // resource and retry with its entity tag, where a 400 would only
            // tell it something was wrong.
            Self::RevisionRequired => StatusCode::PRECONDITION_REQUIRED,

            // 409, as ADR 0008 requires. Not 412: a failed `If-Match` on a
            // read would be 412, but this is a write that was refused because
            // somebody else got there first, and the operator's next step is
            // to redo their edit rather than to correct their header.
            //
            // An integration that moved joins it, for the same reason one
            // level up: a disconnect or another operator's rebind got there
            // first, so the choice has to be made again against what is there
            // now. Not a 400 — the request was well-formed and would have been
            // applied a moment earlier — and not a 503, which would advertise
            // an immediate retry that would be refused identically.
            //
            // `CatalogueRevisionConflict` joins it for the reason its own
            // rustdoc gives: same event, different document, same remedy.
            //
            // `ClientExists` and `RealmUnavailable` join them at 409 for a
            // related but distinct reason: not "read again and redo it" but
            // "this name is already taken", which is why each carries its
            // own code below rather than `revision_conflict`.
            Self::RevisionConflict
            | Self::CatalogueRevisionConflict
            | Self::IntegrationMoved
            | Self::ClientExists { .. }
            | Self::RealmUnavailable { .. } => StatusCode::CONFLICT,

            // 422, not 413: 413 describes a request body that is itself too
            // large, but the request here can be a small edit — removing one
            // role — against a document that is already big for reasons
            // that edit did not create. What is unprocessable is the
            // document the write would produce, which is exactly what 422
            // says, and not retryable: GitHub's contents API cannot read
            // this document back once it is this large, so nothing about
            // waiting and asking again would help.
            Self::DocumentTooLarge { .. } => StatusCode::UNPROCESSABLE_ENTITY,

            // A stored document that will not parse is the platform's problem,
            // not the caller's, and no retry fixes it -- whether the document
            // is a client's or the catalogue's (a held data-sources document
            // in the same shape shares the code from `platform.rs` instead).
            Self::InvalidDesiredState { .. } | Self::InvalidCatalogue { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }

            // 422, joining `DocumentTooLarge`: understood, and cannot be
            // acted on, so the fix is to change what was submitted.
            // `LogicalDataSourceNotDeclared` shares this (ADR 0023 part 2).
            Self::InvalidDataSource(_) | Self::LogicalDataSourceNotDeclared { .. } => {
                StatusCode::UNPROCESSABLE_ENTITY
            }

            // Three failures, one 503, not all for the same reason. Two are
            // transient and carry `Retry-After`; a platform waiting for an
            // operator does not, retrying will not connect it. The code
            // below tells all three apart.
            Self::RepositoryUnavailable | Self::SignInUnavailable | Self::IntegrationNotConfigured => {
                StatusCode::SERVICE_UNAVAILABLE
            }

            // A pass already holds the publication guard; try again once it
            // releases -- not a wait-out-an-outage 503.
            Self::PublicationRunning => StatusCode::CONFLICT,

            // Five failures, four statuses, and the console branches on the
            // machine code below rather than on any of them. A stale write and
            // a client with no boundary are both 409 and are resolved very
            // differently: one reloads, the other provisions.
            Self::Secrets(secrets) => match secrets {
                crate::SecretsError::NotFound => StatusCode::NOT_FOUND,
                crate::SecretsError::Conflict | crate::SecretsError::NoBoundary => StatusCode::CONFLICT,
                // The store refused *this platform's* credential, so it is a
                // misconfiguration here and not something the operator did.
                crate::SecretsError::Refused => StatusCode::BAD_GATEWAY,
                crate::SecretsError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            },

            // 502, not 503, for all three. A refused credential, a refused
            // request and a Git host that said no are misconfigurations of
            // this platform that will still be refused in five seconds, and
            // advertising any of them as transient would turn one bad
            // credential into a retry storm.
            Self::RepositoryDenied | Self::RepositoryRejected | Self::GitHostRefused => {
                StatusCode::BAD_GATEWAY
            }
        }
    }
}
