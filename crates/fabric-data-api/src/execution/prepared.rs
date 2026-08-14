//! What a prepared operation carries into execution.

use std::sync::Arc;

use fabric_connector::DataConnector;
use fabric_tenant_runtime::ResolvedDataSource;

use crate::models::VisibleFields;
use crate::ResourceDefinition;

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
}
