//! Which status code, and which stable machine code, each failure carries.

use http::StatusCode;

use crate::ControlPlaneError;

impl ControlPlaneError {
    /// The status code the operator's browser sees.
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
            Self::UnknownClient(_) | Self::IntegrationNotManaged | Self::ConvergenceUnavailable => {
                StatusCode::NOT_FOUND
            }
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

    /// A stable machine-readable code, so a client branches on this rather
    /// than on message text.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Unauthenticated(_) => "unauthenticated",
            Self::UnknownClient(_) => "unknown_client",
            Self::InvalidRequest(_) => "invalid_request",
            Self::InvalidDesiredState { .. } => "desired_state_invalid",
            Self::RevisionRequired => "revision_required",
            Self::RevisionConflict => "revision_conflict",
            Self::RealmImmutable { .. } => "realm_immutable",
            Self::RepositoryUnavailable => "repository_unavailable",
            Self::InvalidFlow => "invalid_flow",
            Self::ConvergenceUnavailable => "convergence_unavailable",
            Self::IntegrationNotManaged => "integration_not_managed",
            Self::GitHostRefused => "git_host_refused",
            Self::IntegrationRefused(_) => "integration_refused",
            Self::IntegrationNotConfigured => "integration_not_configured",
            Self::SignInRefused => "sign_in_refused",
            Self::SignInUnavailable => "sign_in_unavailable",
            Self::RepositoryDenied => "repository_denied",
            Self::RepositoryRejected => "repository_rejected",
        }
    }

    /// The message the operator sees.
    ///
    /// Most errors say exactly what they mean — an operator is entitled to
    /// know what the platform is doing. The two that do not are the ones whose
    /// detail comes from outside: a repository failure's `detail` field never
    /// reaches here, and a stored-document failure reports the client and the
    /// validation rule but not the file it came from.
    #[must_use]
    pub fn public_message(&self) -> String {
        self.to_string()
    }
}
