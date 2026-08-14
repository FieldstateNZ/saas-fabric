//! Which status code, and which stable machine code, each failure carries.

use fabric_tenant_runtime::ResolveError;
use http::StatusCode;

use crate::errors::connector_mapping;
use crate::DataApiError;

/// How long a caller is told to wait before repeating a request.
///
/// Deliberately one coarse constant rather than a configuration knob: it is a
/// hint, not a contract, and every client worth the name jitters it. Five
/// seconds is long enough that a connector mid-restart (§35) or a runtime still
/// priming has usually finished, and short enough not to look like a failure.
const RETRY_AFTER_SECONDS: u32 = 5;

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

            // A binding asking for isolation the DataSource cannot provide is
            // a reconciliation error like the two above, and 500 for the same
            // reason: nothing the caller sent is wrong, and no retry will
            // help until an operator fixes the binding. Deliberately not 503
            // -- this does not resolve on its own, and telling a client to
            // retry would turn one misconfigured tenant into a retry storm.
            Self::Resolve(ResolveError::IsolationNotEnforceable { .. }) => StatusCode::INTERNAL_SERVER_ERROR,

            Self::UnknownResource(_) | Self::NotFound => StatusCode::NOT_FOUND,

            // A write the backend only partly applied. 5xx, not 4xx: nothing
            // the caller sent was wrong. 500, not 503: the state is already
            // inconsistent and a retry would re-send rows that did apply, so
            // this must not be advertised as transient. `code()` gives it its
            // own machine code so a client can tell it from the retryable
            // failures rather than inferring from the status.
            Self::PartiallyApplied { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::OperationNotAllowed { .. } | Self::ResourceIsReadOnly { .. } => {
                StatusCode::METHOD_NOT_ALLOWED
            }
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,

            // Every connector failure, including the three transport variants
            // whose answer depends on whether a mutation was in flight. The
            // table is in `connector_mapping` rather than here because it
            // decides a status, a code, *and* a message together, and those
            // three must never describe different beliefs about one failure.
            Self::Connector { error, operation } => connector_mapping::answer(error, *operation).status,
        }
    }

    /// How many seconds the caller should wait before repeating the request.
    ///
    /// Present exactly when the platform is *instructing* a retry, which is
    /// every 503 it emits and nothing else. Deriving it from the status rather
    /// than from a second match is what keeps that true: a failure cannot
    /// acquire a retry hint without also becoming a 503, and no 503 can quietly
    /// lose one.
    ///
    /// This is the header half of the write-path fix. A 502
    /// `write_outcome_unknown` deliberately carries none, because the platform
    /// does not know the retry is safe and must not say otherwise.
    pub(crate) fn retry_after(&self) -> Option<u32> {
        (self.status() == StatusCode::SERVICE_UNAVAILABLE).then_some(RETRY_AFTER_SECONDS)
    }

    /// A stable machine-readable code, so clients branch on this rather than on
    /// message text.
    ///
    /// The connector arm is where this earns its keep. Three transport failures
    /// share a status with something else but need three different client
    /// behaviours — retry, reconcile, or accept that the write landed — and the
    /// code is the only field that can carry that.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Identity(_) => "identity",
            Self::Resolve(ResolveError::RuntimeUnavailable) => "runtime_unavailable",
            Self::Resolve(ResolveError::UnknownTenant(_)) => "unknown_tenant",
            Self::Resolve(
                ResolveError::UnboundDataSource { .. }
                | ResolveError::MissingDataSource { .. }
                | ResolveError::IsolationNotEnforceable { .. },
            ) => "internal",
            Self::UnknownResource(_) => "unknown_resource",
            Self::OperationNotAllowed { .. } => "operation_not_allowed",
            Self::ResourceIsReadOnly { .. } => "read_only",
            Self::Forbidden { .. } => "forbidden",
            Self::BadRequest(_) => "bad_request",
            Self::NotFound => "not_found",
            Self::PartiallyApplied { .. } => "partial_write",
            Self::Connector { error, operation } => connector_mapping::answer(error, *operation).code,
        }
    }
}
