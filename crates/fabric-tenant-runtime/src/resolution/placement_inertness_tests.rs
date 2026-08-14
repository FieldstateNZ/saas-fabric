//! Proof that the request path never makes a placement decision.
//!
//! §17 puts placement in the control plane: reconciliation chooses which
//! DataSource a tenant is bound to, weighing residency, placement class, and
//! whether a DataSource is still taking new tenants. By the time a request
//! arrives that decision is already made and written down in the binding.
//!
//! The runtime therefore reads `placement`, `residency`, and
//! `accepts_new_tenants` for exactly one purpose — carrying them for
//! diagnostics — and must never let any of them change *which* DataSource a
//! request resolves to. The docs say so in several places. These tests are
//! what make it true, because a well-meaning future change ("skip DataSources
//! that aren't accepting tenants", "prefer the in-region one") would read as
//! an obvious improvement and would silently move a tenant's data.
//!
//! Each test below sets up a situation where a resolver *doing* placement
//! would produce a different, superficially more sensible answer than a
//! resolver that just follows the binding.

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_connector::{ConnectorId, FieldName, IsolationModel};
use fabric_core::BindingRevision;

use crate::data_source::{DataResidency, DataSourceCapabilities, PlacementClass, PoolSettings};
use crate::tenant::TenantDataBinding;
use crate::testing::{connection_for, data_source, data_source_id, primary, tenant, tenant_binding};
use crate::{
    DataSource, DataSourceRegistry, ResolveError, RuntimeResolver, TenantRegistry, TenantRuntimeBinding,
};

/// A DataSource whose placement attributes are set to whatever the test needs.
fn placed(
    id: &str,
    placement: PlacementClass,
    residency: DataResidency,
    accepts_new_tenants: bool,
) -> DataSource {
    DataSource {
        id: data_source_id(id),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres").unwrap(),
        // Per id, not a constant: two of these tests register two DataSources,
        // and a shared connection name would make them one physical database —
        // which is a genuine reason to refuse structural isolation and has
        // nothing to do with the placement inertness under test here.
        connection: connection_for(id),
        placement,
        residency,
        pool: PoolSettings::default(),
        capabilities: DataSourceCapabilities {
            writable: true,
            accepts_new_tenants,
        },
        labels: BTreeMap::new(),
    }
}

fn resolver(data_sources: Vec<DataSource>, bound_to: &str) -> RuntimeResolver {
    let tenants = Arc::new(TenantRegistry::new());
    tenants
        .apply_all(vec![tenant_binding("acme", 7, bound_to)])
        .unwrap();

    registry(tenants, data_sources)
}

/// The same, for a tenant on a `Shared` DataSource.
///
/// Structural isolation is refused there — see
/// [`isolation_enforceability_tests`](super) — so a test about shared
/// placement has to use the one model that is safe to share. Nothing about
/// what these tests assert depends on which model it is.
fn resolver_on_shared(data_sources: Vec<DataSource>, bound_to: &str) -> RuntimeResolver {
    let tenants = Arc::new(TenantRegistry::new());
    tenants
        .apply_all(vec![TenantRuntimeBinding::new(
            tenant("acme"),
            BindingRevision::new(7),
        )
        .with_data(
            primary(),
            TenantDataBinding::new(
                data_source_id(bound_to),
                IsolationModel::Discriminator {
                    column: FieldName::try_new("tenant_key").unwrap(),
                    value: "tenant-482".to_owned(),
                },
            ),
        )])
        .unwrap();

    registry(tenants, data_sources)
}

fn registry(tenants: Arc<TenantRegistry>, data_sources: Vec<DataSource>) -> RuntimeResolver {
    let registry = Arc::new(DataSourceRegistry::new());
    registry.apply_all(data_sources).unwrap();

    RuntimeResolver::new(tenants, registry)
}

// -- accepts_new_tenants is a control-plane input, not a request-path gate --

#[test]
fn a_data_source_that_has_stopped_accepting_tenants_still_serves_the_tenants_it_has() {
    // The realistic case: a DataSource is being drained, so reconciliation
    // stops placing *new* tenants on it. Every tenant already bound to it must
    // keep working — draining is not an outage. A resolver that treated this
    // flag as an eligibility check would take down every existing tenant on
    // the DataSource the moment an operator set it.
    let runtime = resolver(
        vec![placed(
            "sql-au-east-03",
            PlacementClass::Dedicated,
            DataResidency::in_region("au-east"),
            false,
        )],
        "sql-au-east-03",
    );

    let resolved = runtime.resolve_data_source(&tenant("acme"), &primary()).unwrap();

    assert_eq!(resolved.data_source.id.as_str(), "sql-au-east-03");
    assert!(!resolved.data_source.capabilities.accepts_new_tenants);
}

#[test]
fn a_closed_data_source_is_not_skipped_in_favour_of_an_open_one() {
    // Both DataSources exist and one is visibly "better" by the placement
    // rule. The binding still wins.
    let runtime = resolver(
        vec![
            placed(
                "sql-au-east-03",
                PlacementClass::Dedicated,
                DataResidency::in_region("au-east"),
                false,
            ),
            placed(
                "sql-au-east-04",
                PlacementClass::Dedicated,
                DataResidency::in_region("au-east"),
                true,
            ),
        ],
        "sql-au-east-03",
    );

    let resolved = runtime.resolve_data_source(&tenant("acme"), &primary()).unwrap();

    assert_eq!(resolved.data_source.id.as_str(), "sql-au-east-03");
}

// -- Residency never re-selects -------------------------------------------

#[test]
fn a_binding_to_an_out_of_region_data_source_is_followed_not_corrected() {
    // A tenant bound to a DataSource in a region that looks wrong is a
    // reconciliation problem, and the control plane's to fix. Quietly serving
    // the request from a different region instead would be the runtime making
    // a residency decision — and would move a tenant's data across a
    // sovereignty boundary to paper over a control-plane bug.
    let runtime = resolver(
        vec![
            placed(
                "sql-eu-west-01",
                PlacementClass::Dedicated,
                DataResidency::in_region("eu-west"),
                true,
            ),
            placed(
                "sql-au-east-03",
                PlacementClass::Dedicated,
                DataResidency::in_region("au-east"),
                true,
            ),
        ],
        "sql-eu-west-01",
    );

    let resolved = runtime.resolve_data_source(&tenant("acme"), &primary()).unwrap();

    assert_eq!(resolved.data_source.id.as_str(), "sql-eu-west-01");
    assert_eq!(resolved.data_source.residency.region, "eu-west");
}

// -- Placement class never re-selects --------------------------------------

#[test]
fn a_binding_to_a_shared_data_source_is_not_upgraded_to_a_dedicated_one() {
    let runtime = resolver_on_shared(
        vec![
            placed(
                "shared-postgres-02",
                PlacementClass::Shared,
                DataResidency::in_region("au-east"),
                true,
            ),
            placed(
                "sql-au-east-03",
                PlacementClass::Dedicated,
                DataResidency::in_region("au-east"),
                true,
            ),
        ],
        "shared-postgres-02",
    );

    let resolved = runtime.resolve_data_source(&tenant("acme"), &primary()).unwrap();

    assert_eq!(resolved.data_source.id.as_str(), "shared-postgres-02");
    assert_eq!(resolved.data_source.placement, PlacementClass::Shared);
}

// -- A dangling binding fails; it does not fall back -----------------------

#[test]
fn a_binding_to_a_missing_data_source_fails_rather_than_choosing_an_available_one() {
    // The sharpest version of the rule. There is exactly one healthy,
    // in-region, open DataSource sitting right there, and the correct
    // behaviour is still to refuse. Picking it would be the runtime placing a
    // tenant — and it would send that tenant's reads and writes to a database
    // that has never held their data.
    let runtime = resolver(
        vec![placed(
            "sql-au-east-04",
            PlacementClass::Dedicated,
            DataResidency::in_region("au-east"),
            true,
        )],
        "sql-au-east-03",
    );

    let error = runtime
        .resolve_data_source(&tenant("acme"), &primary())
        .unwrap_err();

    assert!(matches!(error, ResolveError::MissingDataSource { .. }));
}

// -- None of it reaches the connector --------------------------------------

#[test]
fn the_execution_target_carries_no_placement_attribute() {
    // The `ExecutionTarget` is the whole of what crosses into the connector
    // boundary. Placement being absent from it is what makes the rule
    // structural rather than merely observed: a connector cannot act on a
    // placement class it is never handed.
    //
    // The id here is deliberately neutral — `shared-postgres-02` would make
    // this test pass or fail on whether the *identifier* happens to contain
    // the word "shared", which is a property of the name an operator chose,
    // not of what the target carries.
    let runtime = resolver_on_shared(
        vec![placed(
            "pg-02",
            PlacementClass::Shared,
            DataResidency::in_region("eu-west"),
            false,
        )],
        "pg-02",
    );

    // Over the target's own `Debug`, not over `physical_resource_identifier`.
    // That formatter prints four named fields, none of which is placement, so
    // searching it for "shared" could never have found anything — the test
    // passed because of what the *formatter* omits rather than what the type
    // does. Rendering the whole value fails the day `ExecutionTarget` grows a
    // placement, residency, or capability field, which is the regression.
    let resolved = runtime.resolve_data_source(&tenant("acme"), &primary()).unwrap();
    let rendered = format!("{:?}", resolved.target).to_lowercase();

    assert!(!rendered.contains("shared"), "{rendered}");
    assert!(!rendered.contains("dedicated"), "{rendered}");
    assert!(!rendered.contains("eu-west"), "{rendered}");
    assert!(!rendered.contains("accepts"), "{rendered}");
}

// -- The control-plane inputs are still readable ---------------------------

#[test]
fn placement_attributes_remain_available_for_diagnostics() {
    // Inert on the request path is not the same as absent. Reconciliation and
    // operators still need to see these, and a test that only proved the
    // negative would be satisfied by deleting the fields.
    let runtime = resolver(vec![data_source("sql-au-east-03", 4)], "sql-au-east-03");

    let resolved = runtime.resolve_data_source(&tenant("acme"), &primary()).unwrap();

    assert_eq!(resolved.data_source.placement, PlacementClass::Dedicated);
    assert_eq!(resolved.data_source.residency.region, "au-east");
    assert!(resolved.data_source.capabilities.accepts_new_tenants);
}
