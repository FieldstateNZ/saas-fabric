//! Tenants, DataSources, and the catalogue the tests run against.
//!
//! Two tenants on deliberately different placements: `acme` has a dedicated
//! database, `globex` shares a table with a discriminator. One API, two
//! placements, which is the point of §18.

use std::collections::BTreeMap;

use fabric_connector::{ConnectionName, ConnectionSelector, ConnectorId, FieldName, IsolationModel};
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_data_api::ResourceCatalog;
use fabric_identity::TrustedIssuer;
use fabric_tenant_runtime::{
    DataResidency, DataSource, DataSourceCapabilities, PlacementClass, PoolSettings, TenantDataBinding,
    TenantRuntimeBinding,
};

/// Every tenant any suite in this crate names in a `tenant_id` claim.
///
/// `ghost` is deliberately here and deliberately absent from [`tenants`]: the
/// failure-mode suites need a tenant the *identity* layer will bind and the
/// *runtime registry* will not know, which is what `unknown_tenant` is about.
/// If it were unregistered here it would be refused as a credential instead,
/// and those suites would stop testing what they were written to test.
const REGISTERED_TENANTS: [&str; 6] = ["acme", "globex", "ghost", "orphan", "reader", "stayer"];

/// A validated field name.
pub fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}

/// A validated tenant id.
pub fn tenant(name: &str) -> TenantId {
    TenantId::try_new(name).unwrap()
}

/// The issuer registered to `tenant` in the test registry.
pub fn issuer_for(tenant: &str) -> String {
    format!("https://identity.test.invalid/realms/{tenant}")
}

/// The identity registry these suites run against: one issuer per tenant.
///
/// One each, not one shared issuer, because `tenant_isolation` drives `acme`
/// **and** `globex` through a single resolver — and a registration binds one
/// issuer to one tenant, so a shared issuer could only ever name one of them.
/// That is the production shape too: one runtime service, many tenant realms.
pub fn trusted_issuers() -> Vec<TrustedIssuer> {
    REGISTERED_TENANTS
        .iter()
        .map(|name| TrustedIssuer::new(issuer_for(name), tenant(name)))
        .collect()
}

/// The issuer a token claiming `tenant` should carry, for a test that does not
/// care which issuer minted it.
///
/// Where the claim names a registered tenant, that tenant's issuer — so an
/// ordinary request resolves the tenant it asked for. Where it does not (no
/// tenant claim at all, or a value that is not an identifier), `acme`'s
/// registration, so the token still comes from a *registered* issuer and the
/// test exercises the refusal it was written for rather than an unregistered
/// issuer refused one step earlier.
pub fn issuer_naming(tenant: Option<&str>) -> String {
    match tenant {
        Some(name) if REGISTERED_TENANTS.contains(&name) => issuer_for(name),
        _ => issuer_for("acme"),
    }
}

/// Builds a DataSource with the given connection.
fn data_source(id: &str, revision: u64, connection: &str, placement: PlacementClass) -> DataSource {
    DataSource {
        id: DataSourceId::try_new(id).unwrap(),
        revision: BindingRevision::new(revision),
        connector: ConnectorId::try_new("postgres").unwrap(),
        connection: ConnectionSelector::Named {
            name: ConnectionName::try_new(connection).unwrap(),
        },
        placement,
        residency: DataResidency::in_region("au-east"),
        pool: PoolSettings::default(),
        // Declared, not defaulted: capabilities fail closed, so a writable
        // fixture has to say it is writable.
        capabilities: DataSourceCapabilities {
            writable: true,
            accepts_new_tenants: true,
        },
        labels: BTreeMap::new(),
    }
}

/// `acme`'s dedicated database.
pub fn acme_data_source() -> DataSource {
    data_source("acme-prod", 4, "acme-prod", PlacementClass::Dedicated)
}

/// The shared DataSource `globex` sits on.
pub fn discriminator_data_source() -> DataSource {
    data_source("shared-02", 9, "shared-02", PlacementClass::Shared)
}

/// A DataSource the platform will not accept writes to.
pub fn read_only_data_source() -> DataSource {
    DataSource {
        capabilities: DataSourceCapabilities {
            writable: false,
            accepts_new_tenants: true,
        },
        // Dedicated, not shared: the tenant bound to it below uses
        // `Database` isolation, which contributes no predicate, so a shared
        // DataSource could not enforce it and resolution refuses the pair.
        // Nothing about read-only capability depends on placement.
        ..data_source("replica-01", 1, "replica-01", PlacementClass::Dedicated)
    }
}

/// A DataSource that is draining: writable, but closed to new placement.
///
/// The pair that must never be conflated — draining is a control-plane state
/// and has to leave existing traffic untouched.
pub fn draining_data_source() -> DataSource {
    DataSource {
        capabilities: DataSourceCapabilities {
            writable: true,
            accepts_new_tenants: false,
        },
        // Dedicated for the same reason as `replica-01` above: draining is a
        // capability state and is orthogonal to placement.
        ..data_source("draining-01", 1, "draining-01", PlacementClass::Dedicated)
    }
}

/// A tenant already bound to the draining DataSource.
pub fn tenant_on_draining() -> TenantRuntimeBinding {
    TenantRuntimeBinding::new(tenant("stayer"), BindingRevision::new(1)).with_data(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBinding::new(
            DataSourceId::try_new("draining-01").unwrap(),
            IsolationModel::Database,
        ),
    )
}

/// The standard DataSource set.
pub fn data_sources() -> Vec<DataSource> {
    vec![acme_data_source(), discriminator_data_source()]
}

/// The standard tenant set.
pub fn tenants() -> Vec<TenantRuntimeBinding> {
    let primary = LogicalDataSourceName::try_new("primary").unwrap();

    let acme = TenantRuntimeBinding::new(tenant("acme"), BindingRevision::new(7)).with_data(
        primary.clone(),
        TenantDataBinding::new(
            DataSourceId::try_new("acme-prod").unwrap(),
            IsolationModel::Database,
        ),
    );

    let globex = TenantRuntimeBinding::new(tenant("globex"), BindingRevision::new(3)).with_data(
        primary,
        TenantDataBinding::new(
            DataSourceId::try_new("shared-02").unwrap(),
            IsolationModel::Discriminator {
                column: field("tenant_key"),
                value: "tenant-482".to_owned(),
            },
        ),
    );

    vec![acme, globex]
}

/// A tenant bound to a DataSource that is not registered.
pub fn tenant_with_missing_data_source() -> TenantRuntimeBinding {
    TenantRuntimeBinding::new(tenant("orphan"), BindingRevision::new(1)).with_data(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBinding::new(
            DataSourceId::try_new("never-deployed").unwrap(),
            IsolationModel::Database,
        ),
    )
}

/// A tenant bound to the read-only DataSource.
pub fn tenant_on_replica() -> TenantRuntimeBinding {
    TenantRuntimeBinding::new(tenant("reader"), BindingRevision::new(1)).with_data(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBinding::new(
            DataSourceId::try_new("replica-01").unwrap(),
            IsolationModel::Database,
        ),
    )
}

/// The catalogue: one writable resource, one read-only, one with a field
/// allowlist.
///
/// `restrictedCustomers` is the only entry with a non-empty
/// `queryable_fields`, and it exists for one purpose: without it, every field
/// name is permitted, so no test could tell "the resource exposes this field"
/// apart from "the resource exposes everything". It is what makes the
/// authorization-ordering suite able to ask whether an unauthorised caller can
/// distinguish a real field from an invented one.
pub fn catalog() -> ResourceCatalog {
    serde_json::from_str(
        r#"{
            "customers": {
                "data_source": "primary",
                "collection": "customers",
                "operations": ["read", "list", "create", "update", "delete"]
            },
            "readOnlyReport": {
                "data_source": "primary",
                "collection": "customers"
            },
            "restrictedCustomers": {
                "data_source": "primary",
                "collection": "customers",
                "operations": ["read", "list", "create", "update", "delete"],
                "queryable_fields": ["id", "name"]
            }
        }"#,
    )
    .unwrap()
}
