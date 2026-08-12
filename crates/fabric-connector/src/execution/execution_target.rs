//! The fully resolved physical destination for one operation.

use fabric_core::{BindingRevision, TenantId};

use crate::{ConnectionSelector, ConnectorId, IsolationModel};

/// Everything the execution layer needs to know about *where* an operation
/// runs.
///
/// This is the output of tenant resolution and the input to a
/// [`DataConnector`](crate::DataConnector). Producing one is the end of the
/// chain the platform owns:
///
/// ```text
/// bearer token → tenant_id → TenantRuntimeBinding → logical data source
///              → ExecutionTarget → connector
/// ```
///
/// It is carried separately from the operation itself
/// ([`QuerySpec`](crate::QuerySpec)) on purpose: the operation describes *what*
/// the caller asked for and could in principle be logged verbatim, while this
/// describes *where* it goes and is internal (§29).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionTarget {
    tenant: TenantId,
    revision: BindingRevision,
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
        connector: ConnectorId,
        connection: ConnectionSelector,
        isolation: IsolationModel,
    ) -> Self {
        Self {
            tenant,
            revision,
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

    /// The binding revision this target was resolved from.
    ///
    /// Emitted in telemetry (§29) and used to detect that a target was resolved
    /// from a binding that has since been replaced.
    #[must_use]
    pub const fn revision(&self) -> BindingRevision {
        self.revision
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
    /// Specification §29 lists `physical_resource_identifier` as a telemetry
    /// field and requires that it normally stay inside platform telemetry.
    /// Nothing sensitive is included — connector id, connection *label*, and
    /// isolation model, never a credential.
    #[must_use]
    pub fn physical_resource_identifier(&self) -> String {
        format!(
            "{}/{}/{}",
            self.connector,
            self.connection.telemetry_label(),
            self.isolation.telemetry_label()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionName, SchemaName, SecretRef};

    fn target(connection: ConnectionSelector, isolation: IsolationModel) -> ExecutionTarget {
        ExecutionTarget::new(
            TenantId::try_new("acme").unwrap(),
            BindingRevision::new(42),
            ConnectorId::try_new("postgres-au-east").unwrap(),
            connection,
            isolation,
        )
    }

    #[test]
    fn the_physical_identifier_describes_the_placement() {
        let target = target(
            ConnectionSelector::Named {
                name: ConnectionName::try_new("shared-02").unwrap(),
            },
            IsolationModel::Schema {
                schema: SchemaName::try_new("acme").unwrap(),
            },
        );

        assert_eq!(
            target.physical_resource_identifier(),
            "postgres-au-east/named:shared-02/schema"
        );
    }

    #[test]
    fn the_physical_identifier_never_contains_a_credential() {
        let target = target(
            ConnectionSelector::Secret {
                reference: SecretRef::new("tenant/acme/data-primary"),
            },
            IsolationModel::Database,
        );

        let identifier = target.physical_resource_identifier();

        // The reference is a path and is safe; there is no resolved value here
        // at all, because a target holds a selector, never a secret.
        assert!(identifier.contains("tenant/acme/data-primary"));
        assert!(!identifier.contains("password"));
    }
}
