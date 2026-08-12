//! Tests for [`ExecutionTarget`](crate::ExecutionTarget).
//!
//! Kept beside the type rather than inside it: the type is a plain record with
//! accessors, and the tests are all about what its telemetry label may and may
//! not contain — a separate concern worth reading on its own.

use fabric_core::{BindingRevision, DataSourceId, TenantId};

use crate::{
    ConnectionName, ConnectionSelector, ConnectorId, ExecutionTarget, IsolationModel, SchemaName, SecretRef,
};

fn target(connection: ConnectionSelector, isolation: IsolationModel) -> ExecutionTarget {
    ExecutionTarget::new(
        TenantId::try_new("acme").unwrap(),
        BindingRevision::new(42),
        DataSourceId::try_new("sql-au-east-03").unwrap(),
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
        "sql-au-east-03/postgres-au-east/named:shared-02/schema"
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

    // The reference is a path and is safe; there is no resolved value here at
    // all, because a target holds a selector, never a secret.
    assert!(identifier.contains("tenant/acme/data-primary"));
    assert!(!identifier.contains("password"));
}

#[test]
fn a_target_carries_both_halves_of_the_resolution_chain() {
    let target = target(ConnectionSelector::Default, IsolationModel::Database);

    // From the DataSource.
    assert_eq!(target.data_source().as_str(), "sql-au-east-03");
    assert_eq!(target.connector().as_str(), "postgres-au-east");
    // From the tenant binding.
    assert_eq!(target.tenant().as_str(), "acme");
    assert_eq!(target.revision(), BindingRevision::new(42));
}
