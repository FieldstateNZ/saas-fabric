//! The error type itself.

use crate::errors::{RefusalDetail, UnsupportedFeature};
use crate::{CollectionName, ConnectorId};

/// Everything that can go wrong executing an operation against a backend.
///
/// These are *expected* outcomes a caller may branch on. The variants are
/// deliberately coarse: the Data API's job is to map them to a status code and
/// a safe message, not to relay a backend's internals to an application.
///
/// # The three transport variants are not interchangeable
///
/// [`Unreachable`](Self::Unreachable), [`OutcomeUnknown`](Self::OutcomeUnknown)
/// and [`ResultLost`](Self::ResultLost) all mean "the HTTP call to the backend
/// failed", and it is tempting to treat them as one thing. They must not be.
/// They differ on the only question a non-idempotent write raises — did it
/// happen? — and
/// [`OperationEffect`](crate::OperationEffect) is where that is spelled out.
/// Read it before mapping any of them to a status code.
#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    /// No connector is registered under this id.
    ///
    /// A configuration error: a tenant's binding names a connector the process
    /// was not started with.
    #[error("no connector is registered with id {0}")]
    UnknownConnector(ConnectorId),

    /// The backend does not support part of the requested operation.
    ///
    /// The operation is refused rather than approximated. Silently dropping a
    /// predicate a backend cannot express would return rows the caller filtered
    /// out, which in a multi-tenant system is how data crosses a boundary.
    ///
    /// This is the one variant with two audiences: `feature` is the only
    /// connector error text `fabric-data-api` forwards to an application, while
    /// every sibling here is masked. [`UnsupportedFeature`] and
    /// [`RefusalDetail`] are where that split is enforced and explained; build
    /// a refusal through [`UnsupportedFeature::refused`] rather than by hand.
    #[error("connector does not support {feature}")]
    Unsupported {
        /// The capability that was required but is not available. Published to
        /// the caller, so it may name no physical resource.
        feature: UnsupportedFeature,
        /// Which collection, field, or procedure it was needed for. Internal
        /// telemetry only.
        detail: RefusalDetail,
    },

    /// The collection does not exist in the backend's schema.
    #[error("collection {0} does not exist in the connector schema")]
    UnknownCollection(CollectionName),

    /// The referenced credential could not be resolved.
    #[error("could not resolve secret {reference}")]
    SecretUnavailable {
        /// The reference that failed to resolve. The reference is safe to
        /// report; the value it points at would not be.
        reference: String,
    },

    /// The request never reached the backend, so **nothing was carried out**.
    ///
    /// The narrow variant: a refused or timed-out connect, a name that would
    /// not resolve, a request that could not be built. Every one of those
    /// fails before a byte of the request is written, which is what makes this
    /// the only transport failure a caller may safely retry.
    ///
    /// A failure that happens *after* the request is on the wire is
    /// [`OutcomeUnknown`](Self::OutcomeUnknown), not this.
    #[error("connector {connector} is unreachable")]
    Unreachable {
        /// Which connector failed.
        connector: ConnectorId,
        /// The transport-level cause, for logs.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The request went out and no complete answer came back, so it **may or
    /// may not** have been carried out.
    ///
    /// A total-request timeout that fired after the body was sent, a connection
    /// reset mid-flight, a peer that closed without a status line. The backend
    /// may have executed the whole operation and lost the answer on the way
    /// back.
    ///
    /// Retrying a non-idempotent write on this is how a write gets applied
    /// twice. See [`OperationEffect`](crate::OperationEffect).
    #[error("connector {connector} did not answer; the operation may have been carried out")]
    OutcomeUnknown {
        /// Which connector failed.
        connector: ConnectorId,
        /// The transport-level cause, for logs.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The backend reported success and then the answer was lost in transit.
    ///
    /// A success status line was read off the wire — so the operation ran and
    /// **did** take effect — and the body failed part-way through. Only the
    /// result is missing, not the effect. Retrying would apply it a second
    /// time.
    #[error("connector {connector} succeeded but its response was lost in transit")]
    ResultLost {
        /// Which connector failed.
        connector: ConnectorId,
        /// The transport-level cause, for logs.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The backend rejected the operation.
    ///
    /// Carries the backend's own message for the log. The Data API must not
    /// pass this to an application verbatim: it can name physical tables,
    /// servers, and schemas, which §2 and §29 keep internal.
    #[error("connector {connector} rejected the operation with {status}: {message}")]
    Rejected {
        /// Which connector rejected it.
        connector: ConnectorId,
        /// The status the backend answered with, as a plain `u16` so this crate
        /// names no transport type. [`rejection_effect`](crate::rejection_effect)
        /// owns its reading, and argues why the number rather than the verdict
        /// is what crosses this boundary.
        status: u16,
        /// The backend's message. Internal telemetry only.
        message: String,
    },

    /// The backend's response could not be understood.
    ///
    /// Only ever built after a *success* status, so the operation took effect —
    /// see [`OperationEffect`](crate::OperationEffect).
    #[error("connector {connector} returned a malformed response: {detail}")]
    MalformedResponse {
        /// Which connector responded.
        connector: ConnectorId,
        /// What was wrong with it.
        detail: String,
    },

    /// The operation as constructed is not valid.
    ///
    /// A platform bug or a catalogue misconfiguration, not a backend failure.
    #[error("invalid operation: {0}")]
    InvalidOperation(String),
}
