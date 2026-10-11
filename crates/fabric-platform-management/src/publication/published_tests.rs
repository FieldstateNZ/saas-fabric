//! Reading back what an accepted tenants document carries.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    IsolationModelDocument, TenantBindingDocument, TenantDataBindingDocument, TenantDataBindings,
};

use super::published_tenants;

fn tenant(value: &str) -> TenantId {
    TenantId::try_new(value).unwrap()
}

#[test]
fn published_tenants_reads_each_binding_revision() {
    let data = TenantDataBindings::try_new(BTreeMap::from([(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new("shared-a").unwrap(),
            isolation: IsolationModelDocument::Database {},
        },
    )]))
    .unwrap();
    let binding = TenantBindingDocument {
        tenant: tenant("acme"),
        revision: BindingRevision::new(4),
        data,
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    };

    assert_eq!(
        published_tenants(&[binding]),
        BTreeMap::from([(tenant("acme"), BindingRevision::new(4))])
    );
}
