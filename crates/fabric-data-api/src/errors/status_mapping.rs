//! Which status code, and which stable machine code, each failure carries.

use fabric_connector::ConnectorError;
use fabric_tenant_runtime::ResolveError;
use http::StatusCode;

use crate::DataApiError;

impl DataApiError {
    /// The status code the caller sees.
    ///
    /// Several arms map to the same status while meaning very different things
    /// — an unknown tenant and a scope refusal are both 403, for opposite
    /// reasons. They stay separate so each can carry its own reasoning;
    /// collapsing them to satisfy the lint would delete the explanation.
    #[allow(clippy::match_same_arms)]
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Identity(error) => error.status(),

            // A cold or broken runtime is a platform problem, so 503 — and 503
            // is retryable, which is exactly right here.
            Self::Resolve(ResolveError::RuntimeUnavailable) => StatusCode::SERVICE_UNAVAILABLE,

            // An unknown tenant is 403, not 404. The tenant was authenticated;
            // it simply has nothing here. 404 would let a caller probe which
            // tenants exist by watching status codes.
            Self::Resolve(ResolveError::UnknownTenant(_)) => StatusCode::FORBIDDEN,

            // Both remaining resolution failures are reconciliation gaps on the
            // platform's side, not caller errors.
            Self::Resolve(ResolveError::UnboundDataSource { .. }) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Resolve(ResolveError::MissingDataSource { .. }) => StatusCode::INTERNAL_SERVER_ERROR,

            Self::UnknownResource(_) | Self::NotFound => StatusCode::NOT_FOUND,
            Self::OperationNotAllowed { .. } | Self::ResourceIsReadOnly { .. } => {
                StatusCode::METHOD_NOT_ALLOWED
            }
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,

            // A connector we could not reach is not a defect in this process,
            // and 500 tells the caller the wrong thing about it: 500 says
            // "something here is broken, retrying will not help", when in
            // fact the connector may be mid-restart and the very next attempt
            // may succeed. 503 says exactly that, and is the status every
            // client library, load balancer, and retry policy already treats
            // as transient. §35 makes this a routine state rather than an
            // exceptional one — a connector that failed startup negotiation
            // stays registered and keeps being retried in the background —
            // so the status a caller sees during that window should be the
            // retryable one.
            //
            // It also matches `ResolveError::RuntimeUnavailable` above, which
            // is the same shape of problem one layer up: the platform is
            // temporarily unable to serve, not wrong.
            Self::Connector(ConnectorError::Unreachable { .. }) => StatusCode::SERVICE_UNAVAILABLE,

            // The rest of `is_internal` stays 500, and the distinction is
            // deliberate. A malformed response or an unexpected rejection
            // means the connector answered but not in a way we can act on —
            // a version skew, a misconfiguration, a bug. Retrying that
            // reproduces it, so advertising it as transient would send
            // clients into a pointless loop against a fault only an operator
            // can clear.
            Self::Connector(error) if error.is_internal() => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Connector(_) => StatusCode::BAD_REQUEST,
        }
    }

    /// A stable machine-readable code, so clients branch on this rather than on
    /// message text.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Identity(_) => "identity",
            Self::Resolve(ResolveError::RuntimeUnavailable) => "runtime_unavailable",
            Self::Resolve(ResolveError::UnknownTenant(_)) => "unknown_tenant",
            Self::Resolve(
                ResolveError::UnboundDataSource { .. } | ResolveError::MissingDataSource { .. },
            ) => "internal",
            Self::UnknownResource(_) => "unknown_resource",
            Self::OperationNotAllowed { .. } => "operation_not_allowed",
            Self::ResourceIsReadOnly { .. } => "read_only",
            Self::Forbidden { .. } => "forbidden",
            Self::BadRequest(_) => "bad_request",
            Self::NotFound => "not_found",
            Self::Connector(ConnectorError::Unsupported { .. }) => "unsupported",
            Self::Connector(_) => "execution_failed",
        }
    }
}
