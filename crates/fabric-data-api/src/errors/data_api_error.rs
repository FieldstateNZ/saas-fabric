//! Everything the Data API can refuse to do.

use fabric_connector::ConnectorError;
use fabric_core::LogicalResourceName;
use fabric_identity::IdentityError;
use fabric_tenant_runtime::ResolveError;

use crate::OperationKind;

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

    /// The backend applied the write to fewer records than were sent.
    ///
    /// Neither a caller error nor a refusal: the request was well-formed, the
    /// platform accepted it, and the backend applied part of it. It is reported
    /// as a failure rather than a success because the alternative — the
    /// `201 Created` this used to return — tells a caller the batch landed when
    /// some of it did not.
    ///
    /// The counts are safe to publish under rules 1 and 2 above: they describe
    /// the caller's own request and name nothing physical, and a partial
    /// application of *this* tenant's write says nothing about the estate.
    ///
    /// **Not retryable, deliberately.** Records that did apply are still there,
    /// so resending the batch duplicates them. That is why this has its own
    /// machine code rather than sharing `execution_failed`: a client must be
    /// able to tell it apart from the failures where a retry is the right move.
    ///
    /// What the platform cannot add here is *which* records applied — see
    /// `execution::write_integrity` for why no connector capability supplies
    /// that, and why it is not an oversight that can be fixed by asking harder.
    #[error(
        "{applied} of {requested} records were written; the rest were not, and the platform cannot \
         determine which"
    )]
    PartiallyApplied {
        /// How many records the caller sent.
        requested: u64,
        /// How many the backend reported writing.
        applied: u64,
    },

    /// The execution layer failed, carried with the operation it failed for.
    ///
    /// The operation is part of the error rather than a parameter of
    /// [`status`](Self::status) because the answer a transport failure deserves
    /// depends on it, and by the time a response is built the only thing left
    /// is the error. See `errors::connector_mapping` for the table, and note
    /// what its shape rules out: there is no `From<ConnectorError>` for this
    /// type, so a new call site cannot reach a status without stating which
    /// operation it was carrying out.
    #[error("{error}")]
    Connector {
        /// What the connector reported.
        #[source]
        error: ConnectorError,

        /// What was being attempted when it did.
        operation: OperationKind,
    },
}

impl DataApiError {
    /// A connector failure, attributed to the operation that raised it.
    ///
    /// Prefer `Prepared::failed`, which supplies the operation that was
    /// actually authorised and dispatched rather than one a call site
    /// remembered to name. This exists for the one site that fails before a
    /// `Prepared` can be built: looking the connector up at all.
    #[must_use]
    pub(crate) const fn connector(error: ConnectorError, operation: OperationKind) -> Self {
        Self::Connector { error, operation }
    }
}
