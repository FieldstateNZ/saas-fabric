//! Fixture builders shared by this crate's tests.
//!
//! Only compiled under `cfg(test)`. Several test modules need the same
//! `ExecutionTarget`, and duplicating its six arguments across them is how the
//! copies drift when the type gains a field.

use fabric_core::{BindingRevision, DataSourceId, TenantId};

use crate::{CollectionName, ConnectionSelector, ConnectorId, ExecutionTarget, FieldName, IsolationModel};

/// A validated field name.
pub(crate) fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}

/// The collection every fixture operates on.
pub(crate) fn collection() -> CollectionName {
    CollectionName::try_new("customers").unwrap()
}

/// A target with the given isolation model.
pub(crate) fn target_with(isolation: IsolationModel) -> ExecutionTarget {
    ExecutionTarget::new(
        TenantId::try_new("acme").unwrap(),
        BindingRevision::new(1),
        DataSourceId::try_new("shared-02").unwrap(),
        BindingRevision::new(9),
        ConnectorId::try_new("postgres").unwrap(),
        ConnectionSelector::Default,
        isolation,
    )
}

/// A target using discriminator isolation — the case that needs a predicate.
pub(crate) fn discriminator_target() -> ExecutionTarget {
    target_with(IsolationModel::Discriminator {
        column: field("tenant_key"),
        value: "tenant-482".to_owned(),
    })
}
