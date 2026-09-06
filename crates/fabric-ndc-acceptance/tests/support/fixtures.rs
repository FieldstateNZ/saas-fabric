//! The publication fixture every test in the composed acceptance test
//! starts from: one shared DataSource, two tenants isolated by different
//! values in the same discriminator column, and a one-resource catalogue.
//!
//! Deliberately not shared with
//! `fabric-runtime-publication/tests/support/fixtures.rs`, even though the
//! shape rhymes on purpose -- this crate must not depend on that one at all
//! (`src/lib.rs`). The values are the same ones `support::postgres::SEED_SQL`
//! writes as SQL literals, which is what makes the two independent rather
//! than coupled: a mutation to a binding built here cannot move the corpus a
//! real `psql` reads back (`docs/verification.md` row 1a's lesson, applied
//! with a real database instead of a fake corpus).
//!
//! # Built from JSON, not from typed constructors
//!
//! `fabric-runtime-publication`'s own document types are built from
//! `fabric_core` identifiers (`DataSourceId`, `TenantId`, `BindingRevision`,
//! ...), but this crate's `Cargo.toml` deliberately has no `fabric-core`
//! dependency of its own -- the architecture script's dependency-direction
//! table (`scripts/check_architecture.py`) does not list one for
//! `fabric-ndc-acceptance`, and adding one is a `scripts/` change this issue
//! does not make. Every document type here derives `Deserialize` with
//! `#[serde(deny_unknown_fields)]`, so this file builds each one from a
//! `serde_json::json!` literal instead of a struct literal -- the exact
//! bytes a real publisher would accept from an operator's own JSON, which is
//! arguably closer to what "published" means than a typed constructor would
//! be.

use fabric_runtime_publication::{
    CatalogDocument, DataSourceDocument, DocumentInput, DocumentRevision, RuntimeSnapshot,
    TenantBindingDocument,
};
use serde_json::json;

/// The connector id both the published DataSource and the real
/// `NdcConnectorConfig` in every test answer to.
pub const CONNECTOR_ID: &str = "shared-postgres";

/// The one DataSource the fixture's two tenants share.
pub const DATA_SOURCE_ID: &str = "shared-postgres-01";

/// The discriminator column both tenants are isolated on -- the same column
/// `support::postgres::SEED_SQL` names.
pub const DISCRIMINATOR_COLUMN: &str = "tenant_key";

/// acme's value in that column -- the same literal `SEED_SQL` inserts.
pub const ACME_DISCRIMINATOR_VALUE: &str = "tenant-acme-482";

/// globex's value in that column -- the same literal `SEED_SQL` inserts.
pub const GLOBEX_DISCRIMINATOR_VALUE: &str = "tenant-globex-915";

/// The logical name both tenants bind their `primary` data through.
const LOGICAL_PRIMARY: &str = "primary";

/// The logical resource name, which coincides with the physical collection
/// name below -- both `articles`, per the plan's own fixture.
const RESOURCE_NAME: &str = "articles";

/// The one shared DataSource. `writable` gates every write test in this
/// suite; the read-only tests leave it `false`, which is the default a
/// DataSource must be deliberately opted out of (ADR 0004).
pub fn shared_data_source(writable: bool) -> DataSourceDocument {
    serde_json::from_value(json!({
        "id": DATA_SOURCE_ID,
        "revision": 1,
        "connector": CONNECTOR_ID,
        "connection": { "kind": "default" },
        "placement": "shared",
        "residency": { "region": "au-east" },
        "capabilities": { "writable": writable },
    }))
    .unwrap()
}

/// One tenant, discriminator-isolated on the shared DataSource.
pub fn tenant_binding(tenant: &str, discriminator_value: &str) -> TenantBindingDocument {
    serde_json::from_value(json!({
        "tenant": tenant,
        "revision": 1,
        "data": {
            LOGICAL_PRIMARY: {
                "data_source": DATA_SOURCE_ID,
                "isolation": {
                    "kind": "discriminator",
                    "column": DISCRIMINATOR_COLUMN,
                    "value": discriminator_value,
                },
            },
        },
    }))
    .unwrap()
}

/// The one-resource catalogue: `articles`, on the physical collection of the
/// same name (`support::postgres::SEED_SQL`'s table), exposing only `id` and
/// `title` -- never the discriminator column. `operations` is the caller's
/// to set: the read-only tests pass `["read", "list"]`; the write test adds
/// `"create"`.
pub fn articles_catalog(operations: &[&str]) -> CatalogDocument {
    serde_json::from_value(json!({
        RESOURCE_NAME: {
            "data_source": LOGICAL_PRIMARY,
            "collection": RESOURCE_NAME,
            "key_field": "id",
            "operations": operations,
            "queryable_fields": ["id", "title"],
        },
    }))
    .unwrap()
}

/// The read-only snapshot every isolation and fail-closed test starts from:
/// both tenants, the one shared (read-only) DataSource, and a `[read, list]`
/// articles catalogue.
#[must_use]
pub fn read_only_snapshot() -> RuntimeSnapshot {
    RuntimeSnapshot {
        tenants: DocumentInput::new(
            DocumentRevision::new(1),
            vec![
                tenant_binding("acme", ACME_DISCRIMINATOR_VALUE),
                tenant_binding("globex", GLOBEX_DISCRIMINATOR_VALUE),
            ],
        ),
        data_sources: DocumentInput::new(DocumentRevision::new(1), vec![shared_data_source(false)]),
        catalog: DocumentInput::new(DocumentRevision::new(1), articles_catalog(&["read", "list"])),
    }
}

/// The write-enabled twin of [`read_only_snapshot`]: the DataSource declares
/// `writable: true` and the catalogue adds `create` -- the two independent
/// switches ADR 0004 requires before any write is possible at all.
#[must_use]
pub fn writable_snapshot() -> RuntimeSnapshot {
    RuntimeSnapshot {
        tenants: DocumentInput::new(
            DocumentRevision::new(1),
            vec![
                tenant_binding("acme", ACME_DISCRIMINATOR_VALUE),
                tenant_binding("globex", GLOBEX_DISCRIMINATOR_VALUE),
            ],
        ),
        data_sources: DocumentInput::new(DocumentRevision::new(1), vec![shared_data_source(true)]),
        catalog: DocumentInput::new(
            DocumentRevision::new(1),
            articles_catalog(&["read", "list", "create"]),
        ),
    }
}
