//! The fully resolved physical destination for one operation.

use fabric_core::{BindingRevision, DataSourceId, TenantId};

use crate::{ConnectionSelector, ConnectorId, IsolationModel};

/// Everything the execution layer needs to know about *where* an operation
/// runs — and deliberately nothing more.
///
/// This is the output of tenant resolution and the input to a
/// [`DataConnector`](crate::DataConnector). Producing one is the end of the
/// chain the platform owns:
///
/// ```text
/// bearer token → tenant_id → tenant binding → DataSource → ExecutionTarget
/// ```
///
/// It takes both halves of that chain. The DataSource supplies the connector
/// and connection; the tenant binding supplies the isolation. Neither is
/// sufficient alone, which is why a target can only be built by the runtime
/// resolver and never by a caller.
///
/// # What is deliberately absent
///
/// No placement class, no residency, no operational labels, no pool settings,
/// no `accepts_new_tenants`. Those are properties of the
/// [`DataSource`](fabric_tenant_runtime::DataSource) that a connector has no
/// use for, and shipping the whole DataSource down here would turn this type
/// into a transport for platform configuration. It carries identifiers, one
/// connection selector, and the isolation model — the minimum needed to execute
/// safely.
///
/// # Two revisions
///
/// Both are carried, because a request is served by a *pair* of independently
/// reconciled resources and diagnosing it needs both:
///
/// ```text
/// tenant_id=acme tenant_revision=42
/// data_source_id=sql-au-east-03 data_source_revision=7
/// ```
///
/// A pool resize bumps the second and not the first; a tenant rebinding to a
/// different DataSource bumps the first. Recording only one would leave half
/// the changes invisible in a trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionTarget {
    tenant: TenantId,
    tenant_revision: BindingRevision,
    data_source: DataSourceId,
    data_source_revision: BindingRevision,
    connector: ConnectorId,
    connection: ConnectionSelector,
    isolation: IsolationModel,
}

impl ExecutionTarget {
    /// Builds a target. Produced by the tenant runtime, never by a caller.
    #[must_use]
    pub const fn new(
        tenant: TenantId,
        tenant_revision: BindingRevision,
        data_source: DataSourceId,
        data_source_revision: BindingRevision,
        connector: ConnectorId,
        connection: ConnectionSelector,
        isolation: IsolationModel,
    ) -> Self {
        Self {
            tenant,
            tenant_revision,
            data_source,
            data_source_revision,
            connector,
            connection,
            isolation,
        }
    }

    /// The tenant this operation belongs to.
    #[must_use]
    pub const fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// The revision of the tenant binding this target was resolved from.
    ///
    /// Changes when the tenant is rebound to a different DataSource, or its
    /// isolation changes. Does **not** change when the DataSource it points at
    /// is reconfigured.
    #[must_use]
    pub const fn tenant_revision(&self) -> BindingRevision {
        self.tenant_revision
    }

    /// The DataSource this operation runs against.
    #[must_use]
    pub const fn data_source(&self) -> &DataSourceId {
        &self.data_source
    }

    /// The revision of that DataSource's configuration.
    ///
    /// Changes on a pool resize, endpoint correction, credential rebinding or
    /// connector change — none of which touch any tenant's revision.
    #[must_use]
    pub const fn data_source_revision(&self) -> BindingRevision {
        self.data_source_revision
    }

    /// Which connector executes the operation.
    #[must_use]
    pub const fn connector(&self) -> &ConnectorId {
        &self.connector
    }

    /// Which connection within that connector.
    #[must_use]
    pub const fn connection(&self) -> &ConnectionSelector {
        &self.connection
    }

    /// How this tenant's data is isolated.
    #[must_use]
    pub const fn isolation(&self) -> &IsolationModel {
        &self.isolation
    }

    /// An opaque identifier for the physical resource, for internal telemetry.
    ///
    /// §29 lists `physical_resource_identifier` as a telemetry field and
    /// requires that it normally stay inside platform telemetry. Nothing
    /// sensitive is included — DataSource, connector, connection *label*, and
    /// isolation model, never a credential.
    #[must_use]
    pub fn physical_resource_identifier(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.data_source,
            self.connector,
            self.connection.telemetry_label(),
            self.isolation.telemetry_label()
        )
    }
}
