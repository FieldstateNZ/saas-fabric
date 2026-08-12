//! Everything the Data API can refuse to do.

use fabric_connector::ConnectorError;
use fabric_core::LogicalResourceName;
use fabric_identity::IdentityError;
use fabric_tenant_runtime::ResolveError;

/// A refused Data API request.
///
/// Two rules govern what a caller is told, both from §2 and §29, and they are
/// enforced in `response`:
///
/// 1. **Never name physical infrastructure.** No server, database, schema,
///    table, DataSource, or connector identity reaches an application. A
///    connector's own error text often contains several of those, so it is
///    logged and replaced.
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

    /// The DataSource backing this resource does not accept writes.
    ///
    /// Distinct from [`Self::OperationNotAllowed`], which is about the
    /// catalogue. This one is about *placement*: the same catalogue entry is
    /// writable for a tenant on a primary and read-only for a tenant on a
    /// replica. The message deliberately says only "read-only" — which
    /// DataSource, and why, is internal (§2).
    #[error("{resource} is read-only")]
    ResourceIsReadOnly {
        /// The resource addressed.
        resource: String,
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
