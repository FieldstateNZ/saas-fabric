//! Turning transport and status failures into connector errors.

use fabric_connector::{ConnectorError, ConnectorId};

use crate::wire::NdcErrorResponse;

/// Builds a transport error.
pub(super) fn unreachable(connector: &ConnectorId, error: reqwest::Error) -> ConnectorError {
    ConnectorError::Unreachable {
        connector: connector.clone(),
        source: Box::new(error),
    }
}

/// Builds a rejection error, preferring the connector's own message.
///
/// The message is kept for the log. It must not be returned to an application:
/// connector errors name physical tables, schemas, and servers, which §2 and
/// §29 keep internal. The Data API is responsible for that last step, and
/// [`ConnectorError::is_internal`] tells it which errors to replace with a
/// generic message.
pub(super) fn rejected(connector: &ConnectorId, status: reqwest::StatusCode, body: &[u8]) -> ConnectorError {
    let message = serde_json::from_slice::<NdcErrorResponse>(body)
        .map_or_else(|_| format!("connector returned {status}"), |error| error.message);

    ConnectorError::Rejected {
        connector: connector.clone(),
        message,
    }
}

/// Builds a decoding error.
pub(super) fn malformed(connector: &ConnectorId, detail: String) -> ConnectorError {
    ConnectorError::MalformedResponse {
        connector: connector.clone(),
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connector() -> ConnectorId {
        ConnectorId::try_new("postgres").unwrap()
    }

    #[test]
    fn a_structured_error_body_supplies_the_message() {
        let body = br#"{"message":"relation does not exist"}"#;

        let ConnectorError::Rejected { message, .. } =
            rejected(&connector(), reqwest::StatusCode::BAD_REQUEST, body)
        else {
            panic!("expected a rejection");
        };

        assert_eq!(message, "relation does not exist");
    }

    #[test]
    fn an_unparseable_body_falls_back_to_the_status() {
        let ConnectorError::Rejected { message, .. } =
            rejected(&connector(), reqwest::StatusCode::BAD_GATEWAY, b"<html>")
        else {
            panic!("expected a rejection");
        };

        assert!(message.contains("502"));
    }

    #[test]
    fn a_rejection_is_classed_as_internal_so_its_text_never_reaches_a_caller() {
        let error = rejected(&connector(), reqwest::StatusCode::BAD_REQUEST, b"{}");

        assert!(error.is_internal());
    }
}
