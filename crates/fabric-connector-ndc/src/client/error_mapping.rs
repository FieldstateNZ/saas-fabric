//! Turning transport and status failures into connector errors.

use fabric_connector::{ConnectorError, ConnectorId};

use crate::wire::NdcErrorResponse;

/// Builds the right transport error for a failure from `.send()`.
///
/// # The distinction this draws, and why it is not cosmetic
///
/// `.send()` covers everything from resolving a hostname to reading the
/// response's status line, and it reports all of it as one `reqwest::Error`.
/// Two of those outcomes are opposites for a caller holding a non-idempotent
/// write: a connect that was refused never delivered a byte, while a timeout
/// firing after the body went out may have delivered all of it and lost only
/// the answer. Reporting both as "unreachable" tells the caller the write did
/// not happen, and a 503 then invites the retry that applies it twice.
///
/// [`reqwest::Error::is_connect`] is the discriminator. It is true only when
/// the failure came from the connector layer — a refused connection, a connect
/// timeout, a name that would not resolve — all of which run strictly before
/// the request is written. [`reqwest::Error::is_builder`] joins it: a request
/// that could not be constructed was never sent either.
///
/// The classification is deliberately asymmetric. Anything not provably
/// pre-delivery is reported as
/// [`OutcomeUnknown`](ConnectorError::OutcomeUnknown), including cases that
/// really were pre-delivery — a total timeout that happens to fire during the
/// connect phase reports `is_timeout()` without `is_connect()`, and lands here
/// as unknown. That errs toward telling a caller "this may have happened" when
/// it did not, which costs a needless reconciliation. The other direction costs
/// a duplicate write.
pub(super) fn transport_failure(connector: &ConnectorId, error: reqwest::Error) -> ConnectorError {
    if error.is_connect() || error.is_builder() {
        return ConnectorError::Unreachable {
            connector: connector.clone(),
            source: Box::new(error),
        };
    }

    ConnectorError::OutcomeUnknown {
        connector: connector.clone(),
        source: Box::new(error),
    }
}

/// Builds the error for a body that failed to arrive after a success status.
///
/// Separate from [`transport_failure`] because the caller of this one knows
/// something that function cannot: a success status line was already read off
/// the wire. The backend therefore ran the operation and reported that it
/// worked, and only the result was lost — which is a strictly better answer
/// than "may have happened", and the only one that lets the platform tell a
/// caller its write is safe.
pub(super) fn result_lost(connector: &ConnectorId, error: reqwest::Error) -> ConnectorError {
    ConnectorError::ResultLost {
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
        let body = br#"{"message":"relation does not exist","details":{"table":"customers"}}"#;

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
    fn a_body_missing_the_required_details_member_falls_back_to_the_status() {
        // `details` is required by `error_response.jsonschema`, so a body
        // without it is not an NDC error body. Quoting a message out of a
        // document that has just failed to be the shape it claims would be
        // asserting more about it than we know; reporting the status says
        // exactly as much as we do know. A deliberate degradation, and a
        // visible one -- not a silent acceptance of a non-conforming
        // connector.
        let ConnectorError::Rejected { message, .. } = rejected(
            &connector(),
            reqwest::StatusCode::BAD_REQUEST,
            br#"{"message":"relation does not exist"}"#,
        ) else {
            panic!("expected a rejection");
        };

        assert!(message.contains("400"), "{message}");
    }

    #[test]
    fn a_rejection_is_classed_as_internal_so_its_text_never_reaches_a_caller() {
        let error = rejected(&connector(), reqwest::StatusCode::BAD_REQUEST, b"{}");

        assert!(error.is_internal());
    }
}
