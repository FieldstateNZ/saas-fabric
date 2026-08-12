//! Item 51: deletion semantics, proven against the two real resource types.
//!
//! Removal is exercised generically in `apply_tests.rs` and `change_tests.rs`
//! against `TestResource`. This proves the same behaviour holds for
//! `DataSource` and `TenantRuntimeBinding` through the actual `TenantRegistry`
//! / `DataSourceRegistry` aliases: a resource disappearing from a full sync,
//! or being explicitly invalidated, removes runtime access immediately, with
//! no process restart, and is announced as a [`ChangeKind::Removed`] event.
//!
//! The complementary resolver-level guarantee — that a tenant still bound to
//! a DataSource which has since been removed fails closed with
//! `ResolveError::MissingDataSource` rather than falling back — is proven in
//! `resolution/runtime_resolver_tests.rs`, next to the rest of the chain's
//! failure modes.

use crate::resource::ChangeKind;
use crate::testing::{data_source, data_source_id, tenant, tenant_binding};
use crate::{DataSourceRegistry, TenantRegistry};

#[tokio::test]
async fn a_data_source_removed_from_a_full_sync_fails_closed_and_emits_removed() {
    let registry = DataSourceRegistry::new();
    registry.apply_all(vec![data_source("sql-au-east-03", 1)]);
    assert!(registry.lookup(&data_source_id("sql-au-east-03")).is_ok());

    let mut changes = registry.subscribe();
    let report = registry.apply_all(vec![]);

    assert_eq!(report.removed, 1);
    assert!(registry.lookup(&data_source_id("sql-au-east-03")).is_err());

    let change = changes.recv().await.unwrap();
    assert_eq!(change.kind, ChangeKind::Removed);
}

#[tokio::test]
async fn a_tenant_removed_from_a_full_sync_fails_closed_and_emits_removed() {
    let registry = TenantRegistry::new();
    registry.apply_all(vec![tenant_binding("acme", 1, "sql-au-east-03")]);
    assert!(registry.lookup(&tenant("acme")).is_ok());

    let mut changes = registry.subscribe();
    let report = registry.apply_all(vec![]);

    assert_eq!(report.removed, 1);
    assert!(registry.lookup(&tenant("acme")).is_err());

    let change = changes.recv().await.unwrap();
    assert_eq!(change.kind, ChangeKind::Removed);
}

#[test]
fn invalidating_a_data_source_fails_closed_with_no_restart() {
    let registry = DataSourceRegistry::new();
    registry.apply_all(vec![
        data_source("sql-au-east-03", 1),
        data_source("sql-au-east-04", 1),
    ]);

    assert!(registry.invalidate(&data_source_id("sql-au-east-03")));

    assert!(registry.lookup(&data_source_id("sql-au-east-03")).is_err());
    // The registry itself is untouched otherwise — no restart, and the rest
    // of the fleet keeps serving.
    assert!(registry.is_primed());
    assert!(registry.lookup(&data_source_id("sql-au-east-04")).is_ok());
}

#[test]
fn invalidating_a_tenant_fails_closed_with_no_restart() {
    let registry = TenantRegistry::new();
    registry.apply_all(vec![
        tenant_binding("acme", 1, "sql-au-east-03"),
        tenant_binding("globex", 1, "sql-au-east-03"),
    ]);

    assert!(registry.invalidate(&tenant("acme")));

    assert!(registry.lookup(&tenant("acme")).is_err());
    assert!(registry.is_primed());
    assert!(registry.lookup(&tenant("globex")).is_ok());
}
