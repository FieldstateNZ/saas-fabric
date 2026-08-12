//! What a caller is allowed to be told, and how it is sent.

use axum::response::{IntoResponse, Response};
use axum::Json;
use fabric_connector::ConnectorError;
use fabric_tenant_runtime::ResolveError;

use crate::{logging, DataApiError};

impl DataApiError {
    /// The message the caller sees.
    ///
    /// Internal failures collapse to a fixed string. The detail is not lost —
    /// it goes to the log with the trace id — but it does not travel to an
    /// application that has no use for it and should not learn it.
    pub(crate) fn public_message(&self) -> String {
        match self {
            Self::Resolve(ResolveError::RuntimeUnavailable) => {
                "the platform is starting up; retry shortly".to_owned()
            }
            Self::Resolve(ResolveError::UnknownTenant(_)) => "this tenant has no resources here".to_owned(),

            // Both name platform topology — a logical data source the tenant
            // never declared, or a DataSource id. Neither is an application's
            // business (§2).
            Self::Resolve(
                ResolveError::UnboundDataSource { .. } | ResolveError::MissingDataSource { .. },
            ) => "internal error".to_owned(),

            // The connector's own text can name tables, schemas and servers.
            Self::Connector(error) if error.is_internal() => "internal error".to_owned(),
            Self::Connector(ConnectorError::Unsupported { feature }) => {
                format!("this operation is not supported: {feature}")
            }
            Self::Connector(_) => "the request could not be executed".to_owned(),

            other => other.to_string(),
        }
    }
}

impl IntoResponse for DataApiError {
    fn into_response(self) -> Response {
        let status = self.status();

        if status.is_server_error() {
            // Logged here rather than at the throw site, so there is exactly
            // one place every 5xx is recorded with its full internal detail.
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
