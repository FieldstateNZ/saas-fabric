//! Failures at the connector boundary.

use crate::{CollectionName, ConnectorId};

/// Everything that can go wrong executing an operation against a backend.
///
/// These are *expected* outcomes a caller may branch on. The variants are
/// deliberately coarse: the Data API's job is to map them to a status code and
/// a safe message, not to relay a backend's internals to an application.
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
    #[error("connector does not support {feature}")]
    Unsupported {
        /// The capability that was required but is not available.
        feature: String,
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

    /// The backend could not be reached.
    #[error("connector {connector} is unreachable")]
    Unreachable {
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
    #[error("connector {connector} rejected the operation: {message}")]
    Rejected {
        /// Which connector rejected it.
        connector: ConnectorId,
        /// The backend's message. Internal telemetry only.
        message: String,
    },

    /// The backend's response could not be understood.
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

impl ConnectorError {
    /// Whether the failure is the platform's fault rather than the caller's.
    ///
    /// Used to decide between a 5xx and a 4xx, and to decide what may be shown
    /// to the caller. Internal failures get a generic message; the detail goes
    /// to the log.
    #[must_use]
    pub const fn is_internal(&self) -> bool {
        matches!(
            self,
            Self::UnknownConnector(_)
                | Self::SecretUnavailable { .. }
                | Self::Unreachable { .. }
                | Self::MalformedResponse { .. }
                | Self::Rejected { .. }
        )
    }
}
