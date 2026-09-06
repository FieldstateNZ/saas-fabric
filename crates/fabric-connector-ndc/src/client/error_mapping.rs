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
///
/// The status is carried out as a number rather than only formatted into the
/// fallback message, because it is the only evidence the platform has about
/// whether a refused write ran. Folding it into prose would leave
/// [`ConnectorError::effect`] with a string it must not parse.
/// [`rejection_effect`](fabric_connector::rejection_effect) reads it.
pub(super) fn rejected(connector: &ConnectorId, status: reqwest::StatusCode, body: &[u8]) -> ConnectorError {
    let message = serde_json::from_slice::<NdcErrorResponse>(body)
        .map_or_else(|_| format!("connector returned {status}"), |error| error.message);

    ConnectorError::Rejected {
        connector: connector.clone(),
        status: status.as_u16(),
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
    use fabric_connector::OperationEffect;
    use serde_json::Value;

    use super::*;

    fn connector() -> ConnectorId {
        ConnectorId::try_new("postgres").unwrap()
    }

    /// Reads a real `ndc-postgres` v3.1.0 document, checked in under
    /// `tests/fixtures/` -- see the README there for how it was captured.
    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/ndc-postgres-v3.1.0/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
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
    fn the_status_survives_a_body_that_supplied_its_own_message() {
        // The path that used to lose it. When the body parses, the status never
        // reaches the message string, so carrying it in the variant is the only
        // way `effect()` can see it at all.
        let ConnectorError::Rejected { status, message, .. } = rejected(
            &connector(),
            reqwest::StatusCode::BAD_REQUEST,
            br#"{"message":"unknown column","details":{}}"#,
        ) else {
            panic!("expected a rejection");
        };

        assert_eq!(status, 400);
        assert_eq!(message, "unknown column");
    }

    #[test]
    fn a_refused_request_is_reported_as_certainly_not_applied() {
        // End of the chain this change exists to complete: an NDC 400 becomes a
        // `Rejected` whose `effect()` the Data API can act on.
        let error = rejected(&connector(), reqwest::StatusCode::BAD_REQUEST, b"<html>");

        assert_eq!(error.effect(), OperationEffect::NotApplied);
    }

    #[test]
    fn a_conflict_is_not_reported_as_not_applied_despite_being_4xx() {
        // 409's specification example is a foreign key constraint, raised while
        // writing. Being 4xx does not make it conclusive.
        let error = rejected(&connector(), reqwest::StatusCode::CONFLICT, b"<html>");

        assert_eq!(error.effect(), OperationEffect::Unknown);
    }

    #[test]
    fn a_rejection_is_classed_as_internal_so_its_text_never_reaches_a_caller() {
        let error = rejected(&connector(), reqwest::StatusCode::BAD_REQUEST, b"{}");

        assert!(error.is_internal());
    }

    #[test]
    fn a_parse_error_is_422_with_a_string_details_and_maps_to_rejected() {
        // R2 and A9: a body that is not an NDC request at all (posted
        // straight to `/query`) comes back `422`, and -- unlike every other
        // captured error body -- `details` is a **string**, not `null`.
        // `NdcErrorResponse` places no type constraint on `details`, so both
        // must parse; this pins that the string case actually does.
        //
        // `error-parse-422.json` itself is a reconstruction, not a saved
        // capture: the probe was quoted in the planning document rather than
        // `tee`d to a file at the time (`tests/fixtures/ndc-postgres-v3.1.0/README.md`'s
        // "Two captures reconstructed from the plan's record" section).
        // Transcribed exactly as the plan quoted it, not read back from a
        // byte-for-byte capture.
        let body = fixture("error-parse-422.json");
        let parsed: NdcErrorResponse = serde_json::from_str(&body).unwrap();
        assert!(matches!(parsed.details, Value::String(_)), "{:?}", parsed.details);

        let ConnectorError::Rejected { message, .. } = rejected(
            &connector(),
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
            body.as_bytes(),
        ) else {
            panic!("expected a rejection");
        };
        assert_eq!(message, "Parse error");
    }

    #[test]
    fn an_unknown_operator_is_400_with_null_details() {
        // A9's other half: an operator the connector never declared is `400`
        // with `details: null`, the shape every other captured error body
        // shares except the malformed-body case above.
        let body = fixture("error-unknown-operator.json");
        let parsed: NdcErrorResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed.details, Value::Null);

        let ConnectorError::Rejected { message, .. } =
            rejected(&connector(), reqwest::StatusCode::BAD_REQUEST, body.as_bytes())
        else {
            panic!("expected a rejection");
        };
        assert!(message.contains("equals"), "{message}");
    }

    #[test]
    fn a_mutation_without_a_field_selection_is_refused_by_the_real_connector() {
        // F1's symptom, pinned against the real body: every write this
        // adapter can currently send omits `fields`, and the real connector
        // refuses it outright. The fix (selecting `affected_rows`/`returning`)
        // is slice 5's; this only records that the refusal is real and that
        // it maps to `Rejected`, not to something a caller might retry.
        let body = fixture("mutation-insert-no-fields-400.json");

        let ConnectorError::Rejected { message, .. } =
            rejected(&connector(), reqwest::StatusCode::BAD_REQUEST, body.as_bytes())
        else {
            panic!("expected a rejection");
        };
        assert!(message.contains("affected_rows"), "{message}");
    }
}
