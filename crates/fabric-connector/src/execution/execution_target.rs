//! The fully resolved physical destination for one operation.

use fabric_core::{BindingRevision, DataSourceId, TenantId};

use crate::{ConnectionSelector, ConnectorId, IsolationModel};

/// Everything the execution layer needs to know about *where* an operation
/// runs.
///
/// This is the output of tenant resolution and the input to a
/// [`DataConnector`](crate::DataConnector). Producing one is the end of the
/// chain the platform owns:
///
/// ```text
/// bearer token → tenant_id → tenant binding → DataSource → ExecutionTarget
/// ```
///
/// Note that it takes both halves of that chain. The DataSource supplies the
/// connector and the connection; the tenant binding supplies the isolation.
/// Neither is sufficient alone, which is why a target can only be built by the
/// runtime resolver and never by a caller.
///
/// It is carried separately from the operation itself
/// ([`QuerySpec`](crate::QuerySpec)) on purpose: the operation describes *what*
/// the caller asked for, while this describes *where* it goes and is internal
/// (§29).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionTarget {
    tenant: TenantId,
    revision: BindingRevision,
    data_source: DataSourceId,
    connector: ConnectorId,
    connection: ConnectionSelector,
    isolation: IsolationModel,
}

impl ExecutionTarget {
    /// Builds a target. Produced by the tenant runtime, never by a caller.
    #[must_use]
    pub const fn new(
        tenant: TenantId,
        revision: BindingRevision,
        data_source: DataSourceId,
        connector: ConnectorId,
        connection: ConnectionSelector,
        isolation: IsolationModel,
    ) -> Self {
        Self {
            tenant,
            revision,
            data_source,
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

    /// The tenant binding revision this target was resolved from.
    ///
    /// Emitted in telemetry (§29) and used to detect a target resolved from a
    /// binding that has since been replaced.
    #[must_use]
    pub const fn revision(&self) -> BindingRevision {
        self.revision
    }

    /// The DataSource this operation runs against.
    #[must_use]
    pub const fn data_source(&self) -> &DataSourceId {
        &self.data_source
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
