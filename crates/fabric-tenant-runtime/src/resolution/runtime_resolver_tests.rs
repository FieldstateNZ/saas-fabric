//! The tenant → DataSource chain, end to end.

use std::sync::Arc;

use fabric_core::{BindingRevision, LogicalDataSourceName};

use crate::testing::{data_source, primary, read_only_data_source, tenant, tenant_binding};
use crate::{DataSourceRegistry, ResolveError, RuntimeResolver, TenantRegistry};

/// Builds a resolver over the given state, both registries primed.
fn resolver(
    tenants: Vec<crate::TenantRuntimeBinding>,
    data_sources: Vec<crate::DataSource>,
) -> RuntimeResolver {
    let tenant_registry = Arc::new(TenantRegistry::new());
    tenant_registry.apply_all(tenants);

    let data_source_registry = Arc::new(DataSourceRegistry::new());
    data_source_registry.apply_all(data_sources);

    RuntimeResolver::new(tenant_registry, data_source_registry)
}

fn healthy() -> RuntimeResolver {
    resolver(
        vec![tenant_binding("acme", 7, "sql-au-east-03")],
        vec![data_source("sql-au-east-03", 4)],
    )
}

#[test]
fn resolves_through_the_tenant_binding_to_the_data_source() {
    let resolved = healthy()
        .resolve_data_source(&tenant("acme"), &primary())
        .unwrap();

    assert_eq!(resolved.data_source.id.as_str(), "sql-au-east-03");
    assert_eq!(resolved.target.connector().as_str(), "postgres");
    assert_eq!(resolved.target.data_source().as_str(), "sql-au-east-03");
}

#[test]
fn the_target_carries_the_tenant_binding_revision_not_the_data_source_revision() {
    // The revision reported in telemetry answers "which tenant binding served
    // this request?". The DataSource has its own, independent revision.
    let resolved = healthy()
        .resolve_data_source(&tenant("acme"), &primary())
        .unwrap();

    assert_eq!(resolved.target.tenant_revision(), BindingRevision::new(7));
    assert_eq!(resolved.data_source.revision, BindingRevision::new(4));
}

#[test]
fn two_tenants_can_share_one_data_source() {
    // The reuse the model exists for: one DataSource, many tenants, one copy of
    // its physical configuration.
    let resolver = resolver(
        vec![
            tenant_binding("acme", 1, "shared-01"),
            tenant_binding("globex", 1, "shared-01"),
        ],
        vec![data_source("shared-01", 1)],
    );

    let acme = resolver.resolve_data_source(&tenant("acme"), &primary()).unwrap();
    let globex = resolver
        .resolve_data_source(&tenant("globex"), &primary())
        .unwrap();

    assert_eq!(acme.data_source.id, globex.data_source.id);
    assert_eq!(acme.target.tenant().as_str(), "acme");
    assert_eq!(globex.target.tenant().as_str(), "globex");
}

#[test]
fn a_data_source_change_is_visible_without_touching_any_tenant_binding() {
    // The independence that makes DataSources worth having: correcting a
    // connection is one edit, and bumps one revision.
    let tenant_registry = Arc::new(TenantRegistry::new());
    tenant_registry.apply_all(vec![tenant_binding("acme", 1, "shared-01")]);

    let source_registry = Arc::new(DataSourceRegistry::new());
    source_registry.apply_all(vec![data_source("shared-01", 1)]);

    let resolver = RuntimeResolver::new(Arc::clone(&tenant_registry), Arc::clone(&source_registry));

    let mut moved = data_source("shared-01", 2);
    moved.connector = fabric_connector::ConnectorId::try_new("postgres-replacement").unwrap();
    assert!(source_registry.apply_one(moved));

    let outcome = resolver.resolve_data_source(&tenant("acme"), &primary()).unwrap();

    assert_eq!(outcome.target.connector().as_str(), "postgres-replacement");
    // The tenant binding was never rewritten.
    assert_eq!(outcome.target.tenant_revision(), BindingRevision::new(1));
}

#[test]
fn an_unprimed_runtime_reports_unavailable_rather_than_unknown_tenant() {
    let resolver = RuntimeResolver::new(
        Arc::new(TenantRegistry::new()),
        Arc::new(DataSourceRegistry::new()),
    );

    assert_eq!(
        resolver
            .resolve_data_source(&tenant("acme"), &primary())
            .unwrap_err(),
        ResolveError::RuntimeUnavailable
    );
    assert!(!resolver.is_primed());
}

#[test]
fn an_unknown_tenant_is_rejected() {
    assert_eq!(
        healthy()
            .resolve_data_source(&tenant("ghost"), &primary())
            .unwrap_err(),
        ResolveError::UnknownTenant(tenant("ghost"))
    );
}

#[test]
fn a_logical_name_the_tenant_never_declared_is_rejected_rather_than_falling_back() {
    // The tenant has exactly one binding. Asking for another must not quietly
    // return it — §28 forbids "the first available database".
    let audit = LogicalDataSourceName::try_new("audit").unwrap();

    assert_eq!(
        healthy()
            .resolve_data_source(&tenant("acme"), &audit)
            .unwrap_err(),
        ResolveError::UnboundDataSource {
            tenant: tenant("acme"),
            logical: audit,
        }
    );
}

#[test]
fn a_binding_pointing_at_a_missing_data_source_fails_closed() {
    // Reconciliation error: the tenant references a DataSource the registry
    // does not have. Picking a different one would be a cross-tenant write
    // waiting to happen.
    let resolver = resolver(vec![tenant_binding("acme", 1, "not-deployed")], vec![]);

    let error = resolver
        .resolve_data_source(&tenant("acme"), &primary())
        .unwrap_err();

    assert!(matches!(error, ResolveError::MissingDataSource { .. }));
}

#[test]
fn a_data_source_removed_after_a_successful_resolve_fails_closed_rather_than_falling_back() {
    // Item 51: unlike the test above, the DataSource genuinely existed and
    // this tenant resolved against it. Deprovisioning it afterwards must
    // still fail closed — never silently pick up a different DataSource for
    // a binding that has not itself changed.
    let tenant_registry = Arc::new(TenantRegistry::new());
    tenant_registry.apply_all(vec![tenant_binding("acme", 1, "shared-01")]);

    let source_registry = Arc::new(DataSourceRegistry::new());
    source_registry.apply_all(vec![data_source("shared-01", 1)]);

    let resolver = RuntimeResolver::new(Arc::clone(&tenant_registry), Arc::clone(&source_registry));

    assert!(resolver.resolve_data_source(&tenant("acme"), &primary()).is_ok());

    // The DataSource disappears from a subsequent full sync — deprovisioned,
    // never touching the tenant binding.
    source_registry.apply_all(vec![]);

    let error = resolver
        .resolve_data_source(&tenant("acme"), &primary())
        .unwrap_err();

    assert!(matches!(error, ResolveError::MissingDataSource { .. }));
}

#[test]
fn a_read_only_data_source_is_reported_as_not_writable() {
    let resolver = resolver(
        vec![tenant_binding("acme", 1, "replica-01")],
        vec![read_only_data_source("replica-01")],
    );

    let outcome = resolver.resolve_data_source(&tenant("acme"), &primary()).unwrap();

    assert!(!outcome.is_writable());
}

#[test]
fn readiness_requires_both_registries() {
    let tenant_registry = Arc::new(TenantRegistry::new());
    tenant_registry.apply_all(vec![]);

    let resolver = RuntimeResolver::new(tenant_registry, Arc::new(DataSourceRegistry::new()));

    // Tenants primed, data sources not: the plane cannot serve.
    assert!(!resolver.is_primed());
}

#[test]
fn a_resolved_target_carries_both_revisions() {
    // Item 10/11: a request is served by a pair of independently reconciled
    // resources. Telemetry needs both numbers to answer "which exact
    // configuration served this request?".
    let outcome = healthy()
        .resolve_data_source(&tenant("acme"), &primary())
        .unwrap();

    assert_eq!(outcome.target.tenant_revision(), BindingRevision::new(7));
    assert_eq!(outcome.target.data_source_revision(), BindingRevision::new(4));
}

#[test]
fn a_data_source_revision_bump_does_not_move_the_tenant_revision() {
    let tenant_registry = Arc::new(TenantRegistry::new());
    tenant_registry.apply_all(vec![tenant_binding("acme", 3, "shared-01")]);

    let source_registry = Arc::new(DataSourceRegistry::new());
    source_registry.apply_all(vec![data_source("shared-01", 1)]);

    let resolver = RuntimeResolver::new(Arc::clone(&tenant_registry), Arc::clone(&source_registry));

    // A pool resize, an endpoint correction, a credential rebinding: all of
    // these bump only the DataSource.
    assert!(source_registry.apply_one(data_source("shared-01", 2)));

    let outcome = resolver.resolve_data_source(&tenant("acme"), &primary()).unwrap();

    assert_eq!(outcome.target.data_source_revision(), BindingRevision::new(2));
    assert_eq!(outcome.target.tenant_revision(), BindingRevision::new(3));
}

#[test]
fn rebinding_a_tenant_moves_the_tenant_revision_and_the_data_source_id() {
    let tenant_registry = Arc::new(TenantRegistry::new());
    tenant_registry.apply_all(vec![tenant_binding("acme", 3, "shared-01")]);

    let source_registry = Arc::new(DataSourceRegistry::new());
    source_registry.apply_all(vec![data_source("shared-01", 1), data_source("shared-02", 6)]);

    let resolver = RuntimeResolver::new(Arc::clone(&tenant_registry), Arc::clone(&source_registry));

    assert!(tenant_registry.apply_one(tenant_binding("acme", 4, "shared-02")));

    let outcome = resolver.resolve_data_source(&tenant("acme"), &primary()).unwrap();

    assert_eq!(outcome.target.tenant_revision(), BindingRevision::new(4));
    assert_eq!(outcome.target.data_source().as_str(), "shared-02");
    // And it picks up the target DataSource's own revision, not the old one's.
    assert_eq!(outcome.target.data_source_revision(), BindingRevision::new(6));
}
