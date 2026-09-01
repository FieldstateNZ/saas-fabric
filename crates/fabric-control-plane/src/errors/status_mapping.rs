//! Which status code, and which stable machine code, each failure carries.

use http::StatusCode;

mod codes;
mod messages;

use fabric_platform_management::{DesiredStateError, PlatformError};

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
    /// every one of them has its own.
    ///
    /// One arm here used to be a block for no reason but to keep this lint
    /// quiet. Saying so is better than the trick.
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
            // 404 for both, and they mean different things: one client does
            // not exist, and for this deployment the connection surface does
            // not exist. Neither is a permission problem, which is why neither
            // is a 403 sending an operator to look for a grant.
            Self::UnknownClient(_)
            | Self::IntegrationNotManaged
            | Self::PlatformNotManaged
            | Self::ConvergenceUnavailable => StatusCode::NOT_FOUND,
            // `InvalidFlow` joins these at 400 rather than 401, and that is
            // the interesting one: its caller is a browser the Git host
            // redirected here, holding no identity to be wrong about. What is
            // wrong is the callback, and answering 401 would send an operator
            // round a sign-in loop that cannot end.
            Self::InvalidRequest(_)
            | Self::RealmImmutable { .. }
            | Self::IntegrationRefused(_)
            | Self::InvalidFlow => StatusCode::BAD_REQUEST,

            // 428, not 400. The request is well-formed; what is missing is the
            // precondition that makes it safe to apply, and 428 is the status
            // that says exactly that. A client seeing it knows to read the
            // resource and retry with its entity tag, where a 400 would only
            // tell it something was wrong.
            // Platform Management reached a registry or the platform
            // repository and could not get an answer. 503, not 500: nothing is
            // wrong with the request, desired state is untouched, and the
            // operator's next step is to look again shortly.
            // Nothing is connected. 404 beside the other "this deployment
            // does not have one" answers, because that is what it is: an
            // operator has not connected a platform repository, and their next
            // step is to connect one rather than to retry.
            //
            // Deliberately distinct from the arm below. A *connected*
            // integration that cannot be read is broken and needs looking at;
            // reporting that as "not connected" would send an operator to
            // connect something they already have.
            // Two different absences, one status, and two different *codes* —
            // which is where the distinction an operator acts on lives. Nobody
            // has connected a platform repository, or the environment's
            // manifest does not name this component. Neither is fixed by
            // asking again, so neither is a 503.
            Self::Platform(PlatformError::DesiredState(
                DesiredStateError::NotConnected | DesiredStateError::NotFound { .. },
            )) => StatusCode::NOT_FOUND,

            // 409, not 503 and not 400. The request is well-formed and was
            // understood; the component's state is what does not permit it,
            // and an operator's next step is to look at the policy rather than
            // to retry or to correct their request.
            Self::Platform(PlatformError::NotAdvancing { .. }) => StatusCode::CONFLICT,

            Self::Platform(_) => StatusCode::SERVICE_UNAVAILABLE,

            Self::RevisionRequired => StatusCode::PRECONDITION_REQUIRED,

            // 409, as ADR 0008 requires. Not 412: a failed `If-Match` on a
            // read would be 412, but this is a write that was refused because
            // somebody else got there first, and the operator's next step is
            // to redo their edit rather than to correct their header.
            Self::RevisionConflict => StatusCode::CONFLICT,

            // A stored document that will not parse is the platform's problem,
            // not the caller's, and no retry fixes it.
            Self::InvalidDesiredState { .. } => StatusCode::INTERNAL_SERVER_ERROR,

            // 503 and retryable: Git being briefly unreachable is the ordinary
            // transient failure of this API.
            // Three failures, one status, and they are not the same thing.
            // Two are transient and carry a `Retry-After`; the third is a
            // platform waiting for an operator and deliberately does not —
            // retrying will not connect it. That distinction lives on the
            // error rather than the status (see `response.rs`), and the
            // machine code below is how the console tells them apart.
            Self::RepositoryUnavailable | Self::SignInUnavailable | Self::IntegrationNotConfigured => {
                StatusCode::SERVICE_UNAVAILABLE
            }

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
