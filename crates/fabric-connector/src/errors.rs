//! Failures at the connector boundary.

mod refusal_detail;
mod unsupported_feature;
#[cfg(test)]
mod unsupported_feature_tests;

pub use refusal_detail::RefusalDetail;
pub use unsupported_feature::UnsupportedFeature;

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

    /// This failure as an operator needs to read it, for a log line.
    ///
    /// `Display` is the *safe* rendering: no variant interpolates a
    /// [`RefusalDetail`], so text built from it can go to a caller. This is the
    /// unsafe one, and it exists so that the detail a refusal carries has
    /// exactly one way out and that way is named for where it may go.
    #[must_use]
    pub fn operator_message(&self) -> String {
        match self {
            Self::Unsupported { detail, .. } => match detail.as_str() {
                Some(detail) => format!("{self}: {detail}"),
                None => self.to_string(),
            },
            _ => self.to_string(),
        }
    }
}
