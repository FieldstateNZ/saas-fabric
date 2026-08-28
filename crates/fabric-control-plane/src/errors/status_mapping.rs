//! Which status code, and which stable machine code, each failure carries.

use http::StatusCode;

use crate::ControlPlaneError;

impl ControlPlaneError {
    /// The status code the operator's browser sees.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthenticated(_) => StatusCode::UNAUTHORIZED,
            Self::UnknownClient(_) => StatusCode::NOT_FOUND,
            Self::InvalidRequest(_) | Self::RealmImmutable { .. } => StatusCode::BAD_REQUEST,

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
            Self::RepositoryUnavailable => StatusCode::SERVICE_UNAVAILABLE,

            // 502, not 503, for both. A refused credential and a refused
            // request are misconfigurations of this platform that will still
            // be refused in five seconds, and advertising either as transient
            // would turn one bad secret into a retry storm.
            Self::RepositoryDenied | Self::RepositoryRejected => StatusCode::BAD_GATEWAY,
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
