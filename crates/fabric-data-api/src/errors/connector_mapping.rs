//! What a connector failure becomes on the wire, and why the operation decides.
//!
//! # Why the operation is an input
//!
//! Three of [`ConnectorError`]'s variants say *where in the HTTP exchange the
//! call broke* — before the request was written, after it was written, or after
//! a success status came back. For a read that is a distinction without a
//! difference: nothing was mutated in any of the three, so all three are safe
//! to repeat. For a write it is the only question that matters, because
//! `POST /data/{resource}` carries up to `max_mutation_batch_size` rows and no
//! idempotency key, so it is not a request a client may replay.
//!
//! Collapsing them produced the defect this table closes: a mutation the
//! backend received and applied was reported as `503`, and 503 is the status
//! service meshes and SDK retry policies replay without asking.
//!
//! # Why 502 rather than 500
//!
//! RFC 9110 §15.6.3 is literally accurate for both write-side answers: this
//! process received an invalid response — or none — from an inbound server. It
//! also keeps them apart from 500, which everywhere else here means a fault in
//! *this* process, or a configuration only an operator can repair.
//!
//! It does not make either answer non-retryable — no 5xx is, and the crate
//! README's "What the platform promises about a write" says why. What changes
//! is that the platform stops *instructing* a retry.

use fabric_connector::{ConnectorError, UnsupportedFeature};
use http::StatusCode;

use crate::errors::connector_messages as messages;
use crate::OperationKind;

/// The public message for one connector failure.
pub(super) enum PublicMessage {
    /// A sentence from `connector_messages`, which is where the wording and
    /// the §29 reasoning behind it live.
    Fixed(&'static str),

    /// The one message not fixed in advance: the refused capability's own name,
    /// from [`UnsupportedFeature`]'s closed set rather than connector-supplied
    /// bytes. Carried here so rendering it needs no second match that could
    /// fall out of step with this table.
    Unsupported(UnsupportedFeature),
}

/// Everything a connector failure turns into for the caller.
pub(super) struct ConnectorAnswer {
    /// The status code.
    pub(super) status: StatusCode,
    /// The stable machine code clients branch on.
    pub(super) code: &'static str,
    /// What the caller is told.
    pub(super) message: PublicMessage,
}

/// The code for a transport failure the caller may safely repeat.
const UNAVAILABLE: &str = "connector_unavailable";

/// The code for failures with no client-actionable distinction.
const FAILED: &str = "execution_failed";

/// How this failure is reported, given the operation that raised it.
///
/// One function rather than three, so a status, a code, and a message can never
/// describe different beliefs about the same failure.
pub(super) const fn answer(error: &ConnectorError, operation: OperationKind) -> ConnectorAnswer {
    match (error, operation.is_write()) {
        // Provably not delivered: a refused or timed-out connect, a name that
        // would not resolve, a request that could not be built. Nothing was
        // written, so this is the one transport failure a write may repeat —
        // and the only one that carries a `Retry-After`.
        (ConnectorError::Unreachable { .. }, _) => ConnectorAnswer {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: UNAVAILABLE,
            message: PublicMessage::Fixed(messages::NOT_DELIVERED),
        },

        // The request went out and no answer came back.
        (ConnectorError::OutcomeUnknown { .. }, true) => ConnectorAnswer {
            status: StatusCode::BAD_GATEWAY,
            code: "write_outcome_unknown",
            message: PublicMessage::Fixed(messages::WRITE_UNKNOWN),
        },

        // A success status was read off the wire and the body then died. The
        // rows are in; only the affected count is gone.
        (ConnectorError::ResultLost { .. }, true) => ConnectorAnswer {
            status: StatusCode::BAD_GATEWAY,
            code: "write_result_unavailable",
            message: PublicMessage::Fixed(messages::WRITE_APPLIED),
        },

        // The same two on a read, where the position on the wire tells the
        // caller nothing: no row was changed either way.
        (ConnectorError::OutcomeUnknown { .. } | ConnectorError::ResultLost { .. }, false) => {
            ConnectorAnswer {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: UNAVAILABLE,
                message: PublicMessage::Fixed(messages::READ_INCOMPLETE),
            }
        }

        (ConnectorError::Unsupported { feature, .. }, _) => ConnectorAnswer {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported",
            message: PublicMessage::Unsupported(*feature),
        },

        // Everything the platform masks. The status is fixed; only the wording
        // still depends on whether a mutation was in flight.
        (other, writing) if other.is_internal() => ConnectorAnswer {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: FAILED,
            message: PublicMessage::Fixed(messages::internal(other, writing)),
        },

        _ => ConnectorAnswer {
            status: StatusCode::BAD_REQUEST,
            code: FAILED,
            message: PublicMessage::Fixed(messages::NOT_EXECUTED),
        },
    }
}
