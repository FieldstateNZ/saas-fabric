//! Two publishers over one directory.
//!
//! `FilesystemRuntimePublication::publish` reads what is held, plans, then
//! writes. Two calls that overlap would both plan against the same held
//! state; `publish` now holds a lock across the three steps
//! (`lock.rs`), so a second publisher is refused while the first runs. The
//! tests named `current_behaviour_*` still run the overlapping interleaving
//! through the adapter's own `read_held`, `plan_publication` and
//! `write_if_needed`, without the lock, so the outcome does not depend on
//! thread timing: they pin what a writer that bypasses the lock can still
//! do, which is why ADR 0018's "exactly one writer" still matters.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, TenantId};

use super::held::read_held;
use super::lock::PublicationLock;
use super::paths::DocumentPaths;
use super::write::write_if_needed;
use crate::{
    plan_publication, CatalogDocument, ConnectionName, ConnectionSelectorDocument, ConnectorId,
    DataResidencyDocument, DataSourceCapabilitiesDocument, DataSourceDocument, DocumentInput, DocumentKind,
    DocumentOutcome, DocumentPlan, DocumentRevision, FieldName, FilesystemRuntimePublication,
    IsolationModelDocument, PlacementClassDocument, PoolSettingsDocument, PublicationError,
    ResourceDefinitionDocument, RuntimePublication as _, RuntimeSnapshot, TenantBindingDocument,
    TenantDataBindingDocument, TenantDataBindings,
};

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fabric-runtime-publication-concurrency-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn paths(&self, kind: DocumentKind) -> DocumentPaths {
        DocumentPaths::new(kind, self.0.join(kind_file(kind)))
    }

    fn adapter(&self) -> FilesystemRuntimePublication {
        FilesystemRuntimePublication::new(
            self.0.join("tenants.json"),
            self.0.join("data-sources.json"),
            self.0.join("catalog.json"),
        )
    }

    fn write(&self, kind: DocumentKind, plan: &DocumentPlan) {
        write_if_needed(&self.paths(kind), plan.outcome, &plan.bytes, plan.revision).unwrap();
    }

    fn read(&self, file: &str) -> String {
        std::fs::read_to_string(self.0.join(file)).unwrap()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const fn kind_file(kind: DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Tenants => "tenants.json",
        DocumentKind::DataSources => "data-sources.json",
        DocumentKind::Catalog => "catalog.json",
    }
}

fn data_source(id: &str) -> DataSourceDocument {
    DataSourceDocument {
        id: DataSourceId::try_new(id).unwrap(),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new(id).unwrap(),
        },
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument::default(),
        labels: BTreeMap::new(),
    }
}

fn tenant(id: &str, data_source: &str, revision: u64) -> TenantBindingDocument {
    let mut data = BTreeMap::new();
    data.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new(data_source).unwrap(),
            isolation: IsolationModelDocument::Discriminator {
                column: FieldName::try_new("tenant_key").unwrap(),
                value: id.to_owned(),
            },
        },
    );
    TenantBindingDocument {
        tenant: TenantId::try_new(id).unwrap(),
        revision: BindingRevision::new(revision),
        data: TenantDataBindings::try_new(data).unwrap(),
        configuration: None,
        secrets: None,
        features: BTreeMap::new(),
        storage: BTreeMap::new(),
    }
}

fn catalog() -> CatalogDocument {
    let mut resources = BTreeMap::new();
    resources.insert(
        LogicalResourceName::try_new("customers").unwrap(),
        ResourceDefinitionDocument {
            data_source: LogicalDataSourceName::try_new("primary").unwrap(),
            collection: crate::CollectionName::try_new("customers").unwrap(),
            key_field: FieldName::try_new("id").unwrap(),
            operations: vec![fabric_core::OperationKind::Read],
            queryable_fields: vec![],
        },
    );
    CatalogDocument::new(resources)
}

fn snapshot(
    tenants: (u64, Vec<TenantBindingDocument>),
    data_sources: (u64, Vec<DataSourceDocument>),
) -> RuntimeSnapshot {
    RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(tenants.0), tenants.1),
        data_sources: DocumentInput::new(DocumentRevision::new(data_sources.0), data_sources.1),
        catalog: DocumentInput::new(DocumentRevision::new(1), catalog()),
    }
}

/// Held: acme on `pg-1`; data sources `pg-1` and `pg-2`, nothing on `pg-2`.
async fn seeded() -> Dir {
    let dir = Dir::new();
    dir.adapter()
        .publish(&snapshot(
            (1, vec![tenant("acme", "pg-1", 1)]),
            (1, vec![data_source("pg-1"), data_source("pg-2")]),
        ))
        .await
        .unwrap();
    dir
}

/// Gap G4 (write skew), fixed for publishers that go through this adapter.
/// Writer A holds the publication lock, as `publish` does from its read to
/// its last write. Writer B's `publish` is refused at once with nothing
/// written. Once A has retired `pg-2` and released the lock, B's offer is
/// planned against what A left and refused, rather than publishing a binding
/// to a DataSource that is gone.
#[tokio::test]
async fn a_second_publisher_is_refused_while_the_first_holds_the_lock() {
    let dir = seeded().await;
    let rebind = snapshot(
        (2, vec![tenant("acme", "pg-2", 2)]),
        (1, vec![data_source("pg-1"), data_source("pg-2")]),
    );
    let tenants_before = dir.read("tenants.json");

    let lock = PublicationLock::acquire(&dir.paths(DocumentKind::Tenants)).unwrap();
    let error = dir.adapter().publish(&rebind).await.unwrap_err();
    assert!(
        matches!(
            error,
            PublicationError::Unwritable {
                document: DocumentKind::Tenants,
                ..
            }
        ),
        "{error}"
    );
    assert!(
        error.to_string().contains("another publication is in progress"),
        "{error}"
    );
    assert_eq!(dir.read("tenants.json"), tenants_before);

    let held = read_held(
        &dir.paths(DocumentKind::Tenants),
        &dir.paths(DocumentKind::DataSources),
        &dir.paths(DocumentKind::Catalog),
    )
    .unwrap();
    let retire = snapshot(
        (1, vec![tenant("acme", "pg-1", 1)]),
        (2, vec![data_source("pg-1")]),
    );
    let a = plan_publication(&retire, &held).unwrap();
    dir.write(DocumentKind::DataSources, &a.data_sources);
    drop(lock);

    let error = dir.adapter().publish(&rebind).await.unwrap_err();
    assert!(matches!(error, PublicationError::StaleRevision { .. }), "{error}");
    assert!(!dir.read("tenants.json").contains("pg-2"));
    assert!(!dir.read("data-sources.json").contains("pg-2"));
}

/// The lock is released when a publication finishes, whether it wrote or
/// was refused, so the next one is not refused for a lock nobody holds.
#[tokio::test]
async fn the_lock_is_released_after_every_publication() {
    let dir = seeded().await;
    let stale = snapshot(
        (1, vec![tenant("acme", "pg-1", 1)]),
        (0, vec![data_source("pg-1"), data_source("pg-2")]),
    );
    let error = dir.adapter().publish(&stale).await.unwrap_err();
    assert!(matches!(error, PublicationError::StaleRevision { .. }), "{error}");

    let next = snapshot(
        (2, vec![tenant("acme", "pg-1", 1), tenant("globex", "pg-2", 1)]),
        (1, vec![data_source("pg-1"), data_source("pg-2")]),
    );
    let report = dir.adapter().publish(&next).await.unwrap();
    assert_eq!(report.tenants, DocumentOutcome::Written);
    drop(PublicationLock::acquire(&dir.paths(DocumentKind::Tenants)).unwrap());
}

/// Gap G4 (write skew), residual: a writer that bypasses the lock. Writer A retires `pg-2`, which is legal because the
/// held tenants document binds nothing to it. Writer B moves acme onto
/// `pg-2`, which is legal because the held data-sources document has it.
/// Each plan is valid against what it read; neither re-checks the document
/// it did not write. Interleaved, the directory ends with acme bound to a
/// DataSource that is no longer published: the `DanglingDataSource` state
/// the plan exists to refuse, reached without either plan refusing it.
#[tokio::test]
async fn current_behaviour_two_publishers_that_bypass_the_lock_can_publish_a_dangling_binding() {
    let dir = seeded().await;
    let held = read_held(
        &dir.paths(DocumentKind::Tenants),
        &dir.paths(DocumentKind::DataSources),
        &dir.paths(DocumentKind::Catalog),
    )
    .unwrap();

    let retire = snapshot(
        (1, vec![tenant("acme", "pg-1", 1)]),
        (2, vec![data_source("pg-1")]),
    );
    let rebind = snapshot(
        (2, vec![tenant("acme", "pg-2", 2)]),
        (1, vec![data_source("pg-1"), data_source("pg-2")]),
    );
    let a = plan_publication(&retire, &held).unwrap();
    let b = plan_publication(&rebind, &held).unwrap();
    assert_eq!(a.data_sources.outcome, DocumentOutcome::Written);
    assert_eq!(a.tenants.outcome, DocumentOutcome::Unchanged);
    assert_eq!(b.data_sources.outcome, DocumentOutcome::Unchanged);
    assert_eq!(b.tenants.outcome, DocumentOutcome::Written);

    dir.write(DocumentKind::DataSources, &a.data_sources);
    dir.write(DocumentKind::DataSources, &b.data_sources);
    dir.write(DocumentKind::Tenants, &b.tenants);
    dir.write(DocumentKind::Tenants, &a.tenants);

    assert!(dir.read("tenants.json").contains("pg-2"));
    assert!(!dir.read("data-sources.json").contains("pg-2"));

    // The published set is one the plan says is unreachable: re-offering
    // writer A's own snapshot is now refused by the retirement guard, and
    // carrying writer B's binding forward is refused as dangling.
    let error = dir.adapter().publish(&retire).await.unwrap_err();
    assert!(
        matches!(error, PublicationError::RetiredDataSourceStillBound { .. }),
        "{error}"
    );
    let next = snapshot(
        (2, vec![tenant("acme", "pg-2", 2)]),
        (3, vec![data_source("pg-1")]),
    );
    let error = dir.adapter().publish(&next).await.unwrap_err();
    assert!(
        matches!(error, PublicationError::DanglingDataSource { .. }),
        "{error}"
    );
}

/// Gap G4 (lost update within one document), residual: writers that bypass
/// the lock. Two writers that both plan the
/// same next revision from the same held state each get `Write`; the second
/// rename silently replaces the first. Neither call reports an error, and the
/// surviving payload carries a revision that two different payloads were
/// published under. The divergence guard (ADR 0018 part 6) only sees writers
/// that run one after the other.
#[tokio::test]
async fn current_behaviour_two_publishers_that_bypass_the_lock_at_one_revision_both_succeed_and_the_last_wins(
) {
    let dir = seeded().await;
    let held = read_held(
        &dir.paths(DocumentKind::Tenants),
        &dir.paths(DocumentKind::DataSources),
        &dir.paths(DocumentKind::Catalog),
    )
    .unwrap();

    let first = snapshot(
        (2, vec![tenant("acme", "pg-1", 1), tenant("globex", "pg-1", 1)]),
        (1, vec![data_source("pg-1"), data_source("pg-2")]),
    );
    let second = snapshot(
        (2, vec![tenant("acme", "pg-1", 1), tenant("initech", "pg-2", 1)]),
        (1, vec![data_source("pg-1"), data_source("pg-2")]),
    );
    let a = plan_publication(&first, &held).unwrap();
    let b = plan_publication(&second, &held).unwrap();
    assert_eq!(a.tenants.outcome, DocumentOutcome::Written);
    assert_eq!(b.tenants.outcome, DocumentOutcome::Written);

    dir.write(DocumentKind::Tenants, &a.tenants);
    dir.write(DocumentKind::Tenants, &b.tenants);

    let held = dir.read("tenants.json");
    assert!(held.contains("initech") && !held.contains("globex"), "{held}");
    assert!(dir.read("tenants.manifest.json").contains("\"revision\": 2"));

    // Sequentially, the same pair is refused: that is the guard the overlap
    // bypasses.
    let error = dir.adapter().publish(&first).await.unwrap_err();
    assert!(
        matches!(
            error,
            PublicationError::DivergentPayload {
                document: DocumentKind::Tenants,
                ..
            }
        ),
        "{error}"
    );
}
