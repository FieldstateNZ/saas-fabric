//! Withholding a tenant's placements from a pass, and reading back what an
//! accepted tenants document carries.

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    IsolationModelDocument, TenantBindingDocument, TenantDataBindingDocument, TenantDataBindings,
};

use super::{publishable, published_tenants};
use crate::PlacementRecord;

fn tenant(value: &str) -> TenantId {
    TenantId::try_new(value).unwrap()
}

fn placement(tenant_id: &str, logical: &str) -> PlacementRecord {
    PlacementRecord {
        tenant: tenant(tenant_id),
        logical: LogicalDataSourceName::try_new(logical).unwrap(),
        revision: BindingRevision::new(1),
        data_source: DataSourceId::try_new("shared-a").unwrap(),
        isolation: IsolationModelDocument::Database {},
        placed_at: "2026-10-11T00:00:00Z".to_owned(),
    }
}

#[test]
fn nothing_withheld_keeps_every_placement() {
    let placements = vec![placement("acme", "primary"), placement("globex", "primary")];

    assert_eq!(publishable(placements.clone(), &BTreeSet::new()), placements);
}

#[test]
fn every_placement_of_a_withheld_tenant_is_left_out() {
    let placements = vec![
        placement("acme", "primary"),
        placement("globex", "primary"),
        placement("globex", "audit"),
    ];

    let kept = publishable(placements, &BTreeSet::from([tenant("globex")]));

    assert_eq!(kept, vec![placement("acme", "primary")]);
}

#[test]
fn withholding_a_tenant_with_no_placements_changes_nothing() {
    let placements = vec![placement("acme", "primary")];

    assert_eq!(
        publishable(placements.clone(), &BTreeSet::from([tenant("initech")])),
        placements
    );
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
