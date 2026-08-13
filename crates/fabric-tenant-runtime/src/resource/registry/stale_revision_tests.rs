//! Item 49: the stale-revision guard, proven against the two real resource
//! types rather than only the generic `TestResource` fixture.
//!
//! `apply_tests.rs` proves the guard once, generically, over the shared
//! lifecycle code. That proof says nothing about whether either concrete
//! type's `RegistryResource` impl actually wires `revision()` to the right
//! field — a copy-paste mistake there (comparing the wrong field, or a
//! `key()`/`revision()` swap) would pass every generic test while still
//! being wrong. These tests exercise `DataSource` and `TenantRuntimeBinding`
//! directly, through the same `TenantRegistry` / `DataSourceRegistry` type
//! aliases the runtime actually uses.

use fabric_core::BindingRevision;

use crate::testing::{data_source, data_source_id, tenant, tenant_binding};
use crate::{DataSourceRegistry, TenantRegistry};

#[test]
fn a_stale_data_source_revision_is_ignored_by_a_full_sync() {
    let registry = DataSourceRegistry::new();
    registry
        .apply_all(vec![data_source("sql-au-east-03", 8)])
        .unwrap();

    let report = registry
        .apply_all(vec![data_source("sql-au-east-03", 7)])
        .unwrap();

    assert_eq!(report.stale_ignored, 1);
    assert_eq!(
        registry
            .lookup(&data_source_id("sql-au-east-03"))
            .unwrap()
            .revision,
        BindingRevision::new(8)
    );
}

#[test]
fn a_stale_data_source_revision_is_ignored_by_apply_one() {
    let registry = DataSourceRegistry::new();
    registry
        .apply_all(vec![data_source("sql-au-east-03", 8)])
        .unwrap();

    assert!(!registry.apply_one(data_source("sql-au-east-03", 7)));
    assert_eq!(
        registry
            .lookup(&data_source_id("sql-au-east-03"))
            .unwrap()
            .revision,
        BindingRevision::new(8)
    );
}

#[test]
fn a_stale_tenant_binding_revision_is_ignored_by_a_full_sync() {
    let registry = TenantRegistry::new();
    registry
        .apply_all(vec![tenant_binding("acme", 8, "sql-au-east-03")])
        .unwrap();

    let report = registry
        .apply_all(vec![tenant_binding("acme", 7, "sql-au-east-03")])
        .unwrap();

    assert_eq!(report.stale_ignored, 1);
    assert_eq!(
        registry.lookup(&tenant("acme")).unwrap().revision,
        BindingRevision::new(8)
    );
}

#[test]
fn a_stale_tenant_binding_revision_is_ignored_by_apply_one() {
    let registry = TenantRegistry::new();
    registry
        .apply_all(vec![tenant_binding("acme", 8, "sql-au-east-03")])
        .unwrap();

    assert!(!registry.apply_one(tenant_binding("acme", 7, "sql-au-east-03")));
    assert_eq!(
        registry.lookup(&tenant("acme")).unwrap().revision,
        BindingRevision::new(8)
    );
}
