//! What the Data API returns when it cannot serve a request.

use axum::response::{IntoResponse, Response};
use axum::Json;
use fabric_connector::ConnectorError;
use fabric_core::LogicalResourceName;
use fabric_identity::IdentityError;
use fabric_tenant_runtime::ResolveError;
use http::StatusCode;

use crate::logging;

/// Everything the Data API can refuse to do.
///
/// # What a caller is told
///
/// Two rules govern the messages here, both from §2 and §29:
///
/// 1. **Never name physical infrastructure.** No server, database, schema,
///    table, or connector identity reaches an application. A connector's own
///    error text often contains all of those, so it is logged and replaced.
/// 2. **Never reveal whether another tenant's data exists.** Errors describe
///    the request, not the estate.
#[derive(Debug, thiserror::Error)]
pub enum DataApiError {
    /// The tenant identity context could not be established.
    #[error(transparent)]
    Identity(#[from] IdentityError),

    /// The tenant's runtime resources could not be resolved.
    #[error(transparent)]
    Resolve(#[from] ResolveError),

    /// No such logical resource is catalogued.
    #[error("no resource named {0}")]
    UnknownResource(LogicalResourceName),

    /// The resource exists but does not expose this operation.
    #[error("{operation} is not available on {resource}")]
    OperationNotAllowed {
        /// The resource addressed.
        resource: String,
        /// The operation attempted.
        operation: &'static str,
    },

    /// The identity is not permitted to perform this operation.
    #[error("not permitted to {operation} {resource}")]
    Forbidden {
        /// The resource addressed.
        resource: String,
        /// The operation attempted.
        operation: &'static str,
    },

    /// The request was malformed.
    #[error("{0}")]
    BadRequest(String),

    /// No row matched.
    #[error("no matching record")]
    NotFound,

    /// The execution layer failed.
    #[error(transparent)]
    Connector(#[from] ConnectorError),
}

impl DataApiError {
    /// The status code the caller sees.
    ///
    /// Several arms map to the same status while meaning very different things
    /// — an unknown tenant and a scope refusal are both 403, but for opposite
    /// reasons. They are kept separate so each can carry its own reasoning;
    /// collapsing them to satisfy the lint would delete the explanation.
    #[allow(clippy::match_same_arms)]
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Identity(error) => error.status(),

            // A cold or broken runtime is a platform problem, so 503 — and
            // 503 is retryable, which is exactly right here.
            Self::Resolve(ResolveError::RuntimeUnavailable) => StatusCode::SERVICE_UNAVAILABLE,

            // An unknown tenant is 403, not 404. The tenant was authenticated;
            // it simply has nothing here. 404 would let a caller probe which
            // tenants exist by watching status codes.
            Self::Resolve(ResolveError::UnknownTenant(_)) => StatusCode::FORBIDDEN,

            // A tenant that never declared this data source is a configuration
            // gap on the platform's side, not a caller error.
            Self::Resolve(ResolveError::UnknownDataSource { .. }) => StatusCode::INTERNAL_SERVER_ERROR,

            Self::UnknownResource(_) | Self::NotFound => StatusCode::NOT_FOUND,
            Self::OperationNotAllowed { .. } => StatusCode::METHOD_NOT_ALLOWED,
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,

            Self::Connector(error) if error.is_internal() => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Connector(_) => StatusCode::BAD_REQUEST,
        }
    }

    /// The message the caller sees.
    ///
    /// Internal failures collapse to a fixed string. The detail is not lost —
    /// it goes to the log with the trace id — but it does not travel to an
    /// application that has no use for it and should not learn it.
    fn public_message(&self) -> String {
        match self {
            Self::Resolve(ResolveError::RuntimeUnavailable) => {
                "the platform is starting up; retry shortly".to_owned()
            }
            Self::Resolve(ResolveError::UnknownTenant(_)) => "this tenant has no resources here".to_owned(),
            Self::Resolve(ResolveError::UnknownDataSource { .. }) => "internal error".to_owned(),

            // The connector's own text can name tables and servers.
            Self::Connector(error) if error.is_internal() => "internal error".to_owned(),
            Self::Connector(ConnectorError::Unsupported { feature }) => {
                format!("this operation is not supported: {feature}")
            }
            Self::Connector(_) => "the request could not be executed".to_owned(),

            other => other.to_string(),
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
            Self::Resolve(ResolveError::UnknownDataSource { .. }) => "internal",
            Self::UnknownResource(_) => "unknown_resource",
            Self::OperationNotAllowed { .. } => "operation_not_allowed",
            Self::Forbidden { .. } => "forbidden",
            Self::BadRequest(_) => "bad_request",
            Self::NotFound => "not_found",
            Self::Connector(ConnectorError::Unsupported { .. }) => "unsupported",
            Self::Connector(_) => "execution_failed",
        }
    }
}

impl IntoResponse for DataApiError {
    fn into_response(self) -> Response {
        let status = self.status();

        if status.is_server_error() {
            // Logged here rather than at the throw site so there is exactly one
            // place every 5xx is recorded, with its full internal detail.
            logging::request_failed(self.code(), &self.to_string());
        }

        let body = serde_json::json!({
            "error": {
                "code": self.code(),
                "message": self.public_message(),
            }
        });

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use fabric_core::TenantId;

    use super::*;

    #[test]
    fn an_unprimed_runtime_is_a_retryable_503() {
        let error = DataApiError::Resolve(ResolveError::RuntimeUnavailable);

        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.code(), "runtime_unavailable");
    }

    #[test]
    fn an_unknown_tenant_is_403_not_404() {
        // 404 would let a caller enumerate which tenants exist.
        let error = DataApiError::Resolve(ResolveError::UnknownTenant(TenantId::try_new("ghost").unwrap()));

        assert_eq!(error.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn an_unknown_tenant_message_does_not_echo_the_tenant_back() {
        let error = DataApiError::Resolve(ResolveError::UnknownTenant(TenantId::try_new("ghost").unwrap()));

        assert!(!error.public_message().contains("ghost"));
    }

    #[test]
    fn a_connector_rejection_never_reaches_the_caller_verbatim() {
        // Connector text names physical tables and servers.
        let error = DataApiError::Connector(ConnectorError::Rejected {
            connector: fabric_connector::ConnectorId::try_new("postgres").unwrap(),
            message: "relation \"acme_prod.customers\" does not exist on sql-au-east-03".to_owned(),
        });

        let message = error.public_message();

        assert_eq!(message, "internal error");
        assert!(!message.contains("acme_prod"));
        assert!(!message.contains("sql-au-east-03"));
    }

    #[test]
    fn an_unsupported_operation_is_explained_because_it_names_no_infrastructure() {
        let error = DataApiError::Connector(ConnectorError::Unsupported {
            feature: "the contains comparison".to_owned(),
        });

        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
        assert!(error.public_message().contains("contains"));
    }

    #[test]
    fn a_missing_data_source_binding_is_reported_as_an_internal_error() {
        // It is a platform configuration gap, and its message would name the
        // tenant's logical topology.
        let error = DataApiError::Resolve(ResolveError::UnknownDataSource {
            tenant: TenantId::try_new("acme").unwrap(),
            data_source: fabric_core::DataSourceName::try_new("audit").unwrap(),
        });

        assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.public_message(), "internal error");
    }

    #[test]
    fn a_missing_tenant_claim_is_a_401() {
        let error = DataApiError::Identity(IdentityError::MissingTenantClaim {
            claim: "tenant_id".to_owned(),
        });

        assert_eq!(error.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn a_tenant_header_attempt_is_a_400() {
        let error = DataApiError::Identity(IdentityError::TenantHeaderPresent {
            header: "x-tenant-id",
        });

        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
    }
}
