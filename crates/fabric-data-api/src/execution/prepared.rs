//! What a prepared operation carries into execution.

use std::sync::Arc;

use fabric_connector::{ConnectorError, DataConnector};
use fabric_tenant_runtime::ResolvedDataSource;

use crate::models::{VisibleFields, WritableFields};
use crate::{DataApiError, OperationKind, ResourceDefinition};

/// Everything one operation needs, once resolution and authorization have
/// succeeded.
///
/// Holding the [`ResolvedDataSource`] rather than just its
/// [`ExecutionTarget`](fabric_connector::ExecutionTarget) is deliberate: the
/// write path has to ask what the *platform* permits on this DataSource, which
/// is a different question from what the connector can express, and both are
/// checked.
pub(super) struct Prepared<'a> {
    /// The catalogue entry being operated on.
    pub(super) resource: &'a ResourceDefinition,

    /// Where the operation runs, and what the DataSource permits.
    pub(super) resolved: ResolvedDataSource,

    /// The connector that will execute it.
    pub(super) connector: Arc<dyn DataConnector>,

    /// What is being carried out.
    ///
    /// Held here rather than passed alongside because a connector failure has
    /// to be attributed to an operation to be answered — a read may repeat any
    /// transport failure, a write may repeat exactly one — and this is the
    /// operation that was actually authorised and dispatched, not one a call
    /// site restated from memory.
    pub(super) operation: OperationKind,
}

impl Prepared<'_> {
    /// What a response for this operation may disclose.
    ///
    /// Both inputs come from here rather than being passed around separately,
    /// because the isolation half is easy to forget: it is not a property of
    /// the resource, it is a property of *where this tenant is placed*, and it
    /// is only knowable once resolution has run. Building the rules from a
    /// `Prepared` means the only way to obtain them is to have resolved and
    /// authorised the operation first.
    pub(super) fn visible_fields(&self) -> VisibleFields<'_> {
        VisibleFields::new(self.resource, self.resolved.target.isolation())
    }

    /// What this operation may write.
    ///
    /// Built from the same two inputs as [`Self::visible_fields`], and for the
    /// same reason: the discriminator half is a property of the tenant's
    /// placement, so it is only knowable once resolution has run. Obtaining
    /// either rule requires a `Prepared`, which requires having resolved and
    /// authorised the operation first.
    pub(super) fn writable_fields(&self) -> WritableFields<'_> {
        WritableFields::new(self.resource, self.resolved.target.isolation())
    }

    /// Attributes a connector failure to the operation that raised it.
    ///
    /// The only way this crate turns a `ConnectorError` into a `DataApiError`
    /// once execution has begun. `DataApiError` has no `From<ConnectorError>`,
    /// so a future write path cannot reach a status without going through here
    /// and saying what it was doing — which is exactly the fact that decides
    /// whether the caller is told to retry.
    pub(super) fn failed(&self, error: ConnectorError) -> DataApiError {
        DataApiError::connector(error, self.operation)
    }
}
