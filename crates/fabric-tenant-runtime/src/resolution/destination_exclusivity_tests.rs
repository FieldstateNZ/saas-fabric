//! Structural isolation, judged on what the runtime observes rather than on
//! what the operator labelled.
//!
//! [ADR 0006] refused `Database` and `Schema` isolation on a DataSource
//! labelled `Shared`. There are six placement classes, and four of the others —
//! `HighAvailability`, `Regulated`, `Development`, `Ephemeral` — assert nothing
//! at all about single tenancy. A clustered Postgres or a dev sandbox serving
//! many tenants is exactly what those labels describe, and the shipped example
//! had one: `initech-dedicated`, `placement: regulated`,
//! `accepts_new_tenants: true`.
//!
//! These tests pin the rule that does not depend on a label being honest —
//! *more than one tenant reaches this destination* — and the two observed
//! facts it is assembled from: which tenants occupy a DataSource, and which
//! DataSources select one connector-and-connection pair.
//!
//! [ADR 0006]: https://github.com/brettsmith/saas-fabric/blob/main/docs/decisions/0006-a-shared-data-source-can-only-serve-discriminator-isolation.md

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_connector::{
    CollectionName, ConnectionSelector, ConnectorId, ExecutionTarget, IsolationModel, QuerySpec,
};
use fabric_core::{BindingRevision, LogicalDataSourceName};

use crate::data_source::{DataResidency, DataSourceCapabilities, PlacementClass, PoolSettings};
use crate::tenant::TenantDataBinding;
use crate::testing::{
    connection_for, data_source_id, primary, shared_tenant_binding, tenant, tenant_binding,
};
use crate::{
    DataSource, DataSourceRegistry, ResolveError, ResolvedDataSource, RuntimeResolver, TenantRegistry,
    TenantRuntimeBinding,
};

/// The four classes that make no single-tenancy claim, plus the two that do.
const EVERY_PLACEMENT: [PlacementClass; 6] = [
    PlacementClass::Shared,
    PlacementClass::Dedicated,
    PlacementClass::HighAvailability,
    PlacementClass::Regulated,
    PlacementClass::Development,
    PlacementClass::Ephemeral,
];

fn data_source(id: &str, placement: PlacementClass, connection: ConnectionSelector) -> DataSource {
    DataSource {
        id: data_source_id(id),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres").unwrap(),
        connection,
        placement,
        residency: DataResidency::in_region("au-east"),
        pool: PoolSettings::default(),
        capabilities: DataSourceCapabilities {
            writable: true,
            accepts_new_tenants: true,
        },
        labels: BTreeMap::new(),
    }
}

/// A DataSource with a connection of its own, which is the safe default shape.
fn distinct(id: &str, placement: PlacementClass) -> DataSource {
    data_source(id, placement, connection_for(id))
}

fn registries(
    tenants: Vec<TenantRuntimeBinding>,
    sources: Vec<DataSource>,
) -> (Arc<TenantRegistry>, Arc<DataSourceRegistry>) {
    let tenant_registry = Arc::new(TenantRegistry::new());
    tenant_registry.apply_all(tenants).unwrap();

    let source_registry = Arc::new(DataSourceRegistry::new());
    source_registry.apply_all(sources).unwrap();

    (tenant_registry, source_registry)
}

fn resolve_one(
    name: &str,
    tenants: Vec<TenantRuntimeBinding>,
    sources: Vec<DataSource>,
) -> Result<ResolvedDataSource, ResolveError> {
    let (tenant_registry, source_registry) = registries(tenants, sources);

    RuntimeResolver::new(tenant_registry, source_registry).resolve_data_source(&tenant(name), &primary())
}

// -- Co-tenancy: more than one tenant on one DataSource ---------------------

#[test]
fn two_tenants_on_one_high_availability_data_source_are_both_refused() {
    // The reviewer's reproduction. `HighAvailability` is not `Shared`, so the
    // placement rule never looked at it — and `Database` isolation contributes
    // no predicate, so both tenants issued a byte-identical unfiltered query
    // over one connection.
    let sources = vec![distinct("pg-ha-01", PlacementClass::HighAvailability)];
    let tenants = vec![
        tenant_binding("acme", 1, "pg-ha-01"),
        tenant_binding("globex", 1, "pg-ha-01"),
    ];

    for name in ["acme", "globex"] {
        let error = resolve_one(name, tenants.clone(), sources.clone()).unwrap_err();

        assert!(
            matches!(error, ResolveError::IsolationNotEnforceable { .. }),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn co_tenancy_refuses_structural_isolation_on_every_placement_class() {
    // Including `Dedicated`, which is the point: the label is a claim, and
    // nothing stops reconciliation binding a second tenant to a DataSource
    // wearing it. Narrowing the old allowlist to `Dedicated` would have moved
    // this hole rather than closing it.
    for placement in EVERY_PLACEMENT {
        let error = resolve_one(
            "acme",
            vec![
                tenant_binding("acme", 1, "pg-01"),
                tenant_binding("globex", 1, "pg-01"),
            ],
            vec![distinct("pg-01", placement)],
        )
        .unwrap_err();

        assert!(
            matches!(error, ResolveError::IsolationNotEnforceable { .. }),
            "two tenants under structural isolation must be refused on {placement:?}: {error:?}"
        );
    }
}

#[test]
fn one_tenant_keeps_structural_isolation_on_a_placement_that_claims_nothing() {
    // The other side of the rule, and why it is keyed on the count rather than
    // on the label: a clustered or regulated DataSource with one tenant on it
    // isolates that tenant perfectly well, and refusing it would make
    // `Database` isolation unusable outside `Dedicated`.
    for placement in EVERY_PLACEMENT {
        let resolved = resolve_one(
            "acme",
            vec![tenant_binding("acme", 1, "pg-01")],
            vec![distinct("pg-01", placement)],
        );

        // `Shared` is still refused on its label alone — ADR 0006's rule, kept.
        if placement == PlacementClass::Shared {
            assert!(resolved.is_err(), "shared placement must still be refused");
        } else {
            assert!(
                resolved.is_ok(),
                "a sole tenant must keep structural isolation on {placement:?}"
            );
        }
    }
}

#[test]
fn a_second_tenant_arriving_on_refresh_closes_the_first_tenants_structural_binding() {
    // Why this is a per-request check over a derived fact rather than a
    // startup cross-scan. The dangerous moment is not boot; it is the refresh
    // an hour later that places a second tenant on a DataSource the first was
    // relying on having to itself.
    let (tenants, sources) = registries(
        vec![tenant_binding("acme", 1, "pg-reg-01")],
        vec![distinct("pg-reg-01", PlacementClass::Regulated)],
    );
    let resolver = RuntimeResolver::new(Arc::clone(&tenants), sources);

    assert!(
        resolver.resolve_data_source(&tenant("acme"), &primary()).is_ok(),
        "precondition: acme has the DataSource to itself"
    );

    assert!(tenants.apply_one(tenant_binding("globex", 1, "pg-reg-01")));

    let error = resolver
        .resolve_data_source(&tenant("acme"), &primary())
        .unwrap_err();

    assert!(
        matches!(error, ResolveError::IsolationNotEnforceable { .. }),
        "acme must stop being served the moment it stops being alone: {error:?}"
    );
}

#[test]
fn a_tenant_bound_twice_to_one_data_source_is_not_co_tenanted_with_itself() {
    // `primary` and `audit` on one database is one tenant reaching its own
    // data twice, which is a legitimate arrangement. Counting bindings rather
    // than distinct tenants would refuse it.
    let audit = LogicalDataSourceName::try_new("audit").unwrap();
    let binding = tenant_binding("acme", 1, "pg-01").with_data(
        audit,
        TenantDataBinding::new(data_source_id("pg-01"), IsolationModel::Database),
    );

    assert!(resolve_one(
        "acme",
        vec![binding],
        vec![distinct("pg-01", PlacementClass::Dedicated)]
    )
    .is_ok());
}

// -- Discriminator isolation is untouched by the rule -----------------------

#[test]
fn many_tenants_may_share_a_data_source_under_discriminator_isolation() {
    // The model shared placement exists for. Neither new rule may narrow it —
    // discriminator isolation carries its own predicate, so a shared
    // connection is safe however many tenants reach it.
    for placement in EVERY_PLACEMENT {
        let resolved = resolve_one(
            "acme",
            vec![
                shared_tenant_binding("acme", 1, "pg-01"),
                shared_tenant_binding("globex", 1, "pg-01"),
                shared_tenant_binding("initech", 1, "pg-01"),
            ],
            vec![distinct("pg-01", placement)],
        );

        assert!(
            resolved.is_ok(),
            "discriminator isolation must work on {placement:?} however many tenants share it"
        );
    }
}

#[test]
fn discriminator_isolation_is_what_keeps_two_tenants_queries_apart() {
    // The contrast that makes the refusal above necessary rather than
    // cautious: with a predicate, two tenants on one connection produce
    // different queries. Without one they do not.
    let sources = vec![distinct("pg-01", PlacementClass::Shared)];
    let tenants = vec![
        shared_tenant_binding("acme", 1, "pg-01"),
        shared_tenant_binding("globex", 1, "pg-01"),
    ];

    let acme = resolve_one("acme", tenants.clone(), sources.clone()).unwrap();
    let globex = resolve_one("globex", tenants, sources).unwrap();
    let query = QuerySpec::new(CollectionName::try_new("customers").unwrap());

    assert_eq!(acme.target.connection(), globex.target.connection());
    assert_ne!(query.for_target(&acme.target), query.for_target(&globex.target));
}

#[test]
fn the_queries_the_refusal_prevents_would_have_been_byte_identical() {
    // Why the refusal is total rather than a logged warning. The targets are
    // built by hand because the resolver will no longer produce them, which is
    // the whole point — this is the request body two tenants used to send.
    let query = QuerySpec::new(CollectionName::try_new("customers").unwrap());
    let target = |name: &str| {
        ExecutionTarget::new(
            tenant(name),
            BindingRevision::new(1),
            data_source_id("pg-ha-01"),
            BindingRevision::new(1),
            ConnectorId::try_new("postgres").unwrap(),
            connection_for("pg-ha-01"),
            IsolationModel::Database,
        )
    };

    assert_eq!(
        query.for_target(&target("acme")),
        query.for_target(&target("globex")),
        "structural isolation contributes nothing to the query, so nothing but the \
         connection could have separated these two tenants"
    );
}

// -- Destination reuse: more than one DataSource on one connection ----------

#[test]
fn two_data_sources_defaulting_on_one_connector_are_one_destination() {
    // The `ConnectionSelector::Default` collapse. Two ids, two revisions, and
    // by that selector's own definition one physical database — so neither may
    // serve structural isolation, even though each has exactly one tenant and
    // both are labelled `Dedicated`.
    let sources = vec![
        data_source("pg-a", PlacementClass::Dedicated, ConnectionSelector::Default),
        data_source("pg-b", PlacementClass::Dedicated, ConnectionSelector::Default),
    ];
    let tenants = vec![
        tenant_binding("acme", 1, "pg-a"),
        tenant_binding("globex", 1, "pg-b"),
    ];

    for name in ["acme", "globex"] {
        let error = resolve_one(name, tenants.clone(), sources.clone()).unwrap_err();

        assert!(
            matches!(error, ResolveError::IsolationNotEnforceable { .. }),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn two_data_sources_naming_one_connection_are_one_destination() {
    // The same fault stated rather than defaulted into. Nothing about the
    // collapse depends on `Default` being the selector.
    let shared = connection_for("pg-primary");
    let error = resolve_one(
        "acme",
        vec![
            tenant_binding("acme", 1, "pg-a"),
            tenant_binding("globex", 1, "pg-b"),
        ],
        vec![
            data_source("pg-a", PlacementClass::Dedicated, shared.clone()),
            data_source("pg-b", PlacementClass::Dedicated, shared),
        ],
    )
    .unwrap_err();

    assert!(matches!(error, ResolveError::IsolationNotEnforceable { .. }));
}

#[test]
fn one_data_source_selecting_the_default_connection_is_still_served() {
    // `Default` is not itself the fault — a connector that really does serve
    // one database is exactly what it is for. Refusing it outright would make
    // the variant unusable rather than merely explicit.
    assert!(resolve_one(
        "acme",
        vec![tenant_binding("acme", 1, "pg-a")],
        vec![data_source(
            "pg-a",
            PlacementClass::Dedicated,
            ConnectionSelector::Default
        )]
    )
    .is_ok());
}

#[test]
fn one_tenant_may_hold_two_data_sources_over_one_destination() {
    // Reuse on its own is not the fault — a tenant with a writable and a
    // read-only DataSource over its own database shares that destination with
    // nobody. A rule that flagged the reuse without asking who occupies the
    // peer would refuse this, which is why the check reads occupancy per peer
    // rather than treating a collision as a verdict.
    let shared = connection_for("acme-db");
    let read_only = LogicalDataSourceName::try_new("reporting").unwrap();

    let binding = tenant_binding("acme", 1, "acme-rw").with_data(
        read_only.clone(),
        TenantDataBinding::new(data_source_id("acme-ro"), IsolationModel::Database),
    );
    let sources = vec![
        data_source("acme-rw", PlacementClass::Dedicated, shared.clone()),
        data_source("acme-ro", PlacementClass::Dedicated, shared),
    ];

    let (tenants, source_registry) = registries(vec![binding], sources);
    let resolver = RuntimeResolver::new(tenants, source_registry);

    assert!(resolver.resolve_data_source(&tenant("acme"), &primary()).is_ok());
    assert!(resolver.resolve_data_source(&tenant("acme"), &read_only).is_ok());
}

#[test]
fn destination_reuse_does_not_disturb_discriminator_isolation() {
    // Two DataSources over one database is a perfectly ordinary way to give
    // two pools or two capability sets to one shared table. Only structural
    // isolation is refused over it.
    let shared = connection_for("pg-primary");

    assert!(resolve_one(
        "acme",
        vec![
            shared_tenant_binding("acme", 1, "pg-a"),
            shared_tenant_binding("globex", 1, "pg-b"),
        ],
        vec![
            data_source("pg-a", PlacementClass::Shared, shared.clone()),
            data_source("pg-b", PlacementClass::Shared, shared),
        ]
    )
    .is_ok());
}
