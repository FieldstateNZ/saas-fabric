//! The trait every execution backend implements.

use async_trait::async_trait;

use crate::{
    ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema, ExecutionTarget, MutationOutcome,
    MutationSpec, QueryOutcome, QuerySpec,
};

/// Executes logical data operations against a physical backend.
///
/// This trait is the whole point of the crate: it is the line above which the
/// platform is protocol-agnostic and below which a specific protocol lives.
/// `fabric-connector-ndc` implements it by speaking NDC over HTTP; a native
/// PostgreSQL provider could implement it by holding a driver pool. Nothing
/// above this trait can tell the difference, which is what keeps that choice
/// reversible.
///
/// # Why `Arc<dyn DataConnector>` rather than a generic
///
/// This is a stateless I/O seam in the sense that matters: no caller ever needs
/// to compose two operations inside a shared transaction *at this level*. The
/// tenant runtime picks a connector by id at request time, so the set of
/// implementations is not known statically anyway, and dynamic dispatch costs
/// nothing measurable next to a network round trip.
///
/// (Contrast a repository over `sqlx`, which must be executor-generic so that
/// `SELECT ... FOR UPDATE` and the following `UPDATE` share a connection.
/// Transactionality here belongs to the connector, which is why
/// [`ConnectorCapabilities::transactional_mutations`] is something a backend
/// *declares* rather than something the platform arranges.)
///
/// # Contract for implementors
///
/// - **Never widen an operation.** If some part of a [`QuerySpec`] cannot be
///   expressed, return [`ConnectorError::Unsupported`]. Do not drop a predicate
///   and return more rows: under discriminator isolation the dropped predicate
///   may be the tenant boundary.
/// - **Honour the target.** Route to
///   [`ExecutionTarget::connection`], and never to a default when the requested
///   connection is unavailable (§28).
/// - **Never log a credential**, and never include one in an error (§29).
/// - **Report an affected-row count that describes the operation sent.** A
///   count above the number of rows handed to
///   [`mutate`](Self::mutate) is not a large success, it is an answer to a
///   different question, and callers treat it as a malformed response. Where a
///   backend gives no usable count, report what it did rather than inventing a
///   plausible one: an under-count is refused as a partial write, which is the
///   safe failure, while a fabricated match is not.
#[async_trait]
pub trait DataConnector: Send + Sync {
    /// The id this connector is registered under.
    fn id(&self) -> &ConnectorId;

    /// What this backend supports.
    ///
    /// Expected to be cached by the implementation. It is consulted on every
    /// operation, so it must not perform I/O per call.
    fn capabilities(&self) -> &ConnectorCapabilities;

    /// The collections this backend exposes.
    ///
    /// Also expected to be cached, and refreshed out of band rather than on the
    /// request path.
    fn schema(&self) -> &ConnectorSchema;

    /// Executes a read.
    ///
    /// The `spec` passed here must already have been through
    /// [`QuerySpec::for_target`] — the tenant predicate is applied there, not
    /// by the connector.
    ///
    /// # Errors
    ///
    /// Any [`ConnectorError`].
    async fn query(&self, target: &ExecutionTarget, spec: &QuerySpec)
        -> Result<QueryOutcome, ConnectorError>;

    /// Executes a write.
    ///
    /// As with [`Self::query`], the `spec` must already have been through
    /// [`MutationSpec::for_target`].
    ///
    /// # Errors
    ///
    /// Any [`ConnectorError`].
    async fn mutate(
        &self,
        target: &ExecutionTarget,
        spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError>;

    /// Checks that the backend is reachable.
    ///
    /// Used by the platform's own readiness probe. A connector that cannot be
    /// reached makes the tenants bound to it unservable, which §28 classes as
    /// service-unavailable rather than as an unknown tenant.
    ///
    /// # Errors
    ///
    /// [`ConnectorError::Unreachable`] if the backend does not respond.
    async fn health(&self) -> Result<(), ConnectorError>;
}
