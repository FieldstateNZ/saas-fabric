//! What a prepared operation carries into execution.

use std::sync::Arc;

use fabric_connector::DataConnector;
use fabric_tenant_runtime::ResolvedDataSource;

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
