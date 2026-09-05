//! The filesystem adapter, proved against a real temporary directory.
//!
//! Every test below owns its own [`TempDir`], never a shared fixture, so
//! tests may run in any order without interfering with each other.

// The crate's atomic-write guarantee is Unix-only, and so is this proof of it.
#![cfg(unix)]
// Clippy's `allow-unwrap-in-tests` only covers `#[test]` functions
// themselves, not the fixture helpers every test here calls into -- an
// integration test file states it once here, as every other one in this
// workspace does.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use fabric_core::{
    BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, OperationKind, TenantId,
};
use fabric_runtime_publication::{
    CatalogDocument, CollectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, DataSourceDocument, DocumentInput, DocumentOutcome, DocumentRevision,
    FieldName, FilesystemRuntimePublication, IsolationModelDocument, PlacementClassDocument,
    PoolSettingsDocument, PublicationError, ResourceDefinitionDocument, RuntimePublication, RuntimeSnapshot,
    TenantBindingDocument, TenantDataBindingDocument, TenantDataBindings,
};
use fabric_tenant_runtime::{
    DataSource as RuntimeDataSource, JsonFileSource, ResourceSource, TenantRuntimeBinding,
};

/// A directory under the system temp root, unique per test, removed when it
/// drops. `tempfile` is not in this workspace's dependency table, so this is
/// the whole of what that crate would otherwise give us.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let unique = format!(
            "fabric-runtime-publication-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn adapter(dir: &TempDir) -> FilesystemRuntimePublication {
    FilesystemRuntimePublication::new(
        dir.path().join("tenants.json"),
        dir.path().join("data-sources.json"),
        dir.path().join("catalog.json"),
    )
}

fn data_source(id: &str, revision: u64) -> DataSourceDocument {
    DataSourceDocument {
        id: DataSourceId::try_new(id).unwrap(),
        revision: BindingRevision::new(revision),
        connector: ConnectorId::try_new("postgres-au-east").unwrap(),
        connection: ConnectionSelectorDocument::Default {},
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "au-east".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument::default(),
        labels: BTreeMap::new(),
    }
}

fn tenant(name: &str, bound_to: &str, revision: u64) -> TenantBindingDocument {
    let mut data = BTreeMap::new();
    data.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new(bound_to).unwrap(),
            isolation: IsolationModelDocument::Database {},
        },
    );

    TenantBindingDocument {
        tenant: TenantId::try_new(name).unwrap(),
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
            collection: CollectionName::try_new("customers").unwrap(),
            key_field: FieldName::try_new("id").unwrap(),
            operations: vec![OperationKind::Read],
            queryable_fields: Vec::new(),
        },
    );
    CatalogDocument::new(resources)
}

/// A self-consistent snapshot: one tenant bound to one DataSource, both at
/// the given revision, plus a fixed catalogue.
fn snapshot(revision: u64, tenant_name: &str, data_source_id: &str) -> RuntimeSnapshot {
    RuntimeSnapshot {
        tenants: DocumentInput::new(
            DocumentRevision::new(revision),
            vec![tenant(tenant_name, data_source_id, 1)],
        ),
        data_sources: DocumentInput::new(
            DocumentRevision::new(revision),
            vec![data_source(data_source_id, 1)],
        ),
        catalog: DocumentInput::new(DocumentRevision::new(revision), catalog()),
    }
}

/// `(inode, modified-time in nanoseconds)`, so a rewrite is detectable even
/// when it happens within the same clock tick as the read it is compared
/// against.
fn identity(path: &Path) -> (u64, i64) {
    let metadata = std::fs::metadata(path).unwrap();
    (metadata.ino(), metadata.mtime_nsec())
}

/// Like [`identity`], but `None` for a file that does not exist -- for
/// asserting a whole set of the six canonical files is untouched when some
/// of them are expected to stay absent both before and after the call under
/// test.
fn existing_identity(path: &Path) -> Option<(u64, i64)> {
    std::fs::metadata(path)
        .ok()
        .map(|metadata| (metadata.ino(), metadata.mtime_nsec()))
}

fn temp_files_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("tmp"))
        })
        .collect()
}

#[tokio::test]
async fn a_first_publication_needs_no_held_manifest() {
    let dir = TempDir::new("first-publication");
    let report = adapter(&dir)
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap();

    assert_eq!(report.tenants, DocumentOutcome::Written);
    assert_eq!(report.data_sources, DocumentOutcome::Written);
    assert_eq!(report.catalog, DocumentOutcome::Written);
    assert!(dir.path().join("tenants.json").exists());
    assert!(dir.path().join("tenants.manifest.json").exists());
}

#[tokio::test]
async fn a_data_source_is_written_before_the_tenant_that_references_it() {
    let dir = TempDir::new("write-order");
    // The data-sources payload's own directory does not exist, so its write
    // fails before catalogue or tenants are even attempted (ADR 0018 part 3:
    // data sources, then catalogue, then tenants).
    let publisher = FilesystemRuntimePublication::new(
        dir.path().join("tenants.json"),
        dir.path().join("missing-directory").join("data-sources.json"),
        dir.path().join("catalog.json"),
    );

    let error = publisher
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap_err();

    assert!(matches!(error, PublicationError::Unwritable { .. }));
    assert!(!dir.path().join("tenants.json").exists());
    assert!(!dir.path().join("catalog.json").exists());
}

/// Three overlapping no-op properties, proved together rather than in three
/// separate tests: a revision that did not advance is not rewritten, a
/// republication with identical bytes at the same revision writes nothing,
/// and "nothing" includes the manifest — not just the payload.
#[tokio::test]
async fn a_republication_at_the_same_revision_with_the_same_payload_writes_nothing() {
    let dir = TempDir::new("same-revision-same-bytes");
    let publisher = adapter(&dir);
    publisher.publish(&snapshot(1, "acme", "sql-01")).await.unwrap();

    let identities_before: Vec<(u64, i64)> = [
        "tenants.json",
        "tenants.manifest.json",
        "data-sources.json",
        "data-sources.manifest.json",
        "catalog.json",
        "catalog.manifest.json",
    ]
    .iter()
    .map(|name| identity(&dir.path().join(name)))
    .collect();

    let report = publisher.publish(&snapshot(1, "acme", "sql-01")).await.unwrap();

    assert_eq!(report.tenants, DocumentOutcome::Unchanged);
    assert_eq!(report.data_sources, DocumentOutcome::Unchanged);
    assert_eq!(report.catalog, DocumentOutcome::Unchanged);

    let identities_after: Vec<(u64, i64)> = [
        "tenants.json",
        "tenants.manifest.json",
        "data-sources.json",
        "data-sources.manifest.json",
        "catalog.json",
        "catalog.manifest.json",
    ]
    .iter()
    .map(|name| identity(&dir.path().join(name)))
    .collect();
    assert_eq!(identities_before, identities_after);
}

#[tokio::test]
async fn a_stale_revision_publication_is_refused_and_the_last_good_files_remain() {
    let dir = TempDir::new("stale-revision");
    let publisher = adapter(&dir);
    publisher.publish(&snapshot(2, "acme", "sql-01")).await.unwrap();

    let before: Vec<(u64, i64)> = ["tenants.json", "tenants.manifest.json"]
        .iter()
        .map(|name| identity(&dir.path().join(name)))
        .collect();

    let error = publisher
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap_err();

    assert!(matches!(error, PublicationError::StaleRevision { .. }));
    let after: Vec<(u64, i64)> = ["tenants.json", "tenants.manifest.json"]
        .iter()
        .map(|name| identity(&dir.path().join(name)))
        .collect();
    assert_eq!(before, after);
}

#[tokio::test]
async fn a_same_revision_publication_with_a_different_payload_is_refused() {
    let dir = TempDir::new("divergent-payload");
    let publisher = adapter(&dir);
    publisher.publish(&snapshot(1, "acme", "sql-01")).await.unwrap();

    // Same revision, different tenant name -- different bytes.
    let error = publisher
        .publish(&snapshot(1, "globex", "sql-01"))
        .await
        .unwrap_err();

    assert!(matches!(error, PublicationError::DivergentPayload { .. }));
}

#[tokio::test]
async fn a_refused_publication_writes_nothing_at_all() {
    let dir = TempDir::new("refused-writes-nothing");
    let publisher = adapter(&dir);
    publisher.publish(&snapshot(2, "acme", "sql-01")).await.unwrap();

    let names = [
        "tenants.json",
        "tenants.manifest.json",
        "data-sources.json",
        "data-sources.manifest.json",
        "catalog.json",
        "catalog.manifest.json",
    ];
    let before: Vec<(u64, i64)> = names
        .iter()
        .map(|name| identity(&dir.path().join(name)))
        .collect();

    let error = publisher
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap_err();

    assert!(matches!(error, PublicationError::StaleRevision { .. }));
    let after: Vec<(u64, i64)> = names
        .iter()
        .map(|name| identity(&dir.path().join(name)))
        .collect();
    assert_eq!(before, after);
    assert!(temp_files_in(dir.path()).is_empty());
}

#[tokio::test]
async fn a_publication_naming_a_data_source_it_does_not_publish_is_refused_before_any_write() {
    let dir = TempDir::new("dangling");
    let snapshot = RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(1), vec![tenant("acme", "missing-ds", 1)]),
        data_sources: DocumentInput::new(DocumentRevision::new(1), vec![]),
        catalog: DocumentInput::new(DocumentRevision::new(1), catalog()),
    };

    let error = adapter(&dir).publish(&snapshot).await.unwrap_err();

    assert!(matches!(error, PublicationError::DanglingDataSource { .. }));
    assert!(!dir.path().join("tenants.json").exists());
    assert!(!dir.path().join("data-sources.json").exists());
    assert!(!dir.path().join("catalog.json").exists());
}

#[tokio::test]
async fn retiring_a_data_source_the_held_tenants_still_bind_is_refused() {
    let dir = TempDir::new("retirement");
    let publisher = adapter(&dir);
    publisher.publish(&snapshot(1, "acme", "sql-01")).await.unwrap();

    let before = identity(&dir.path().join("data-sources.json"));

    // The offered tenants document no longer names the DataSource, so the
    // dangling check has nothing to say -- but the *held* tenants document
    // still binds "acme" to it.
    let retiring = RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(2), vec![]).emptying_intended(),
        data_sources: DocumentInput::new(DocumentRevision::new(2), vec![]).emptying_intended(),
        catalog: DocumentInput::new(DocumentRevision::new(1), catalog()),
    };

    let error = publisher.publish(&retiring).await.unwrap_err();

    assert!(matches!(
        error,
        PublicationError::RetiredDataSourceStillBound { .. }
    ));
    assert_eq!(identity(&dir.path().join("data-sources.json")), before);
}

/// A held data-sources manifest whose payload is gone must refuse the whole
/// publication now, the same way a lost tenants payload already did: the
/// data-sources document feeds its own emptying guard exactly as tenants
/// does, so a lost payload can no longer be silently read as "empty" and
/// republished over. See the catalogue test below for the one document this
/// does *not* apply to, and why.
#[tokio::test]
async fn a_held_data_sources_manifest_without_its_payload_is_refused_as_held_payload_lost() {
    let dir = TempDir::new("data-sources-payload-lost");
    let publisher = adapter(&dir);
    publisher.publish(&snapshot(1, "acme", "sql-01")).await.unwrap();

    std::fs::remove_file(dir.path().join("data-sources.json")).unwrap();

    let error = publisher
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        PublicationError::HeldPayloadLost {
            document: fabric_runtime_publication::DocumentKind::DataSources
        }
    ));
    assert!(!dir.path().join("data-sources.json").exists());
}

/// Unlike data sources (above) and tenants, the catalogue's held state gates
/// no guard at all -- it is only ever byte-compared inside `verdict`, never
/// parsed for a guard -- so a lost payload with its manifest intact is still
/// just a republication for this one document.
#[tokio::test]
async fn a_held_catalogue_manifest_without_its_payload_is_republishable_at_the_same_revision() {
    let dir = TempDir::new("catalog-payload-lost");
    let publisher = adapter(&dir);
    publisher.publish(&snapshot(1, "acme", "sql-01")).await.unwrap();

    std::fs::remove_file(dir.path().join("catalog.json")).unwrap();

    let report = publisher.publish(&snapshot(1, "acme", "sql-01")).await.unwrap();

    assert_eq!(report.catalog, DocumentOutcome::Written);
    assert!(dir.path().join("catalog.json").exists());
}

/// The data-sources twin of
/// `an_emptying_publication_is_refused_when_the_held_tenants_payload_is_lost`
/// below: a held data-sources manifest whose payload is gone refuses an
/// emptying publication too, and unconditionally -- `Emptying::Intended` is
/// stated here and still never gets considered, because the held payload is
/// unreadable before that guard, or any other, ever runs.
#[tokio::test]
async fn an_emptying_publication_is_refused_when_the_held_data_sources_payload_is_lost() {
    let dir = TempDir::new("data-sources-payload-lost-emptying");
    let publisher = adapter(&dir);
    publisher.publish(&snapshot(1, "acme", "sql-01")).await.unwrap();

    std::fs::remove_file(dir.path().join("data-sources.json")).unwrap();

    let emptying = RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(1), vec![tenant("acme", "sql-01", 1)]),
        data_sources: DocumentInput::new(DocumentRevision::new(1), vec![]).emptying_intended(),
        catalog: DocumentInput::new(DocumentRevision::new(1), catalog()),
    };

    let error = publisher.publish(&emptying).await.unwrap_err();

    assert!(matches!(
        error,
        PublicationError::HeldPayloadLost {
            document: fabric_runtime_publication::DocumentKind::DataSources
        }
    ));
    assert!(!dir.path().join("data-sources.json").exists());
}

/// A held tenants manifest whose payload is gone must refuse the whole
/// publication rather than silently treat the held document as empty (B1) --
/// here, via the emptying guard: an empty offering with no
/// `Emptying::Intended` would, before the fix, have gone through because
/// the held length was wrongly read as zero.
#[tokio::test]
async fn an_emptying_publication_is_refused_when_the_held_tenants_payload_is_lost() {
    let dir = TempDir::new("tenants-payload-lost-emptying");
    let publisher = adapter(&dir);
    publisher.publish(&snapshot(1, "acme", "sql-01")).await.unwrap();

    std::fs::remove_file(dir.path().join("tenants.json")).unwrap();

    let emptying = RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(1), vec![]),
        data_sources: DocumentInput::new(DocumentRevision::new(1), vec![data_source("sql-01", 1)]),
        catalog: DocumentInput::new(DocumentRevision::new(1), catalog()),
    };

    let error = publisher.publish(&emptying).await.unwrap_err();

    assert!(matches!(
        error,
        PublicationError::HeldPayloadLost {
            document: fabric_runtime_publication::DocumentKind::Tenants
        }
    ));
    assert!(!dir.path().join("tenants.json").exists());
}

/// The retirement guard reads the held tenants document too (B1): dropping
/// "sql-01" while the held tenants payload is lost must be refused, not
/// silently accepted because the held tenant count was wrongly read as zero.
#[tokio::test]
async fn retiring_a_data_source_is_refused_when_the_held_tenants_payload_is_lost() {
    let dir = TempDir::new("tenants-payload-lost-retirement");
    let publisher = adapter(&dir);
    publisher.publish(&snapshot(1, "acme", "sql-01")).await.unwrap();

    std::fs::remove_file(dir.path().join("tenants.json")).unwrap();

    let retiring = RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(2), vec![]).emptying_intended(),
        data_sources: DocumentInput::new(DocumentRevision::new(2), vec![]).emptying_intended(),
        catalog: DocumentInput::new(DocumentRevision::new(1), catalog()),
    };

    let error = publisher.publish(&retiring).await.unwrap_err();

    assert!(matches!(
        error,
        PublicationError::HeldPayloadLost {
            document: fabric_runtime_publication::DocumentKind::Tenants
        }
    ));
    assert!(dir.path().join("data-sources.json").exists());
}

#[tokio::test]
async fn a_manifest_naming_the_wrong_document_kind_is_refused() {
    let dir = TempDir::new("manifest-kind-mismatch");
    adapter(&dir)
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap();

    // The manifest's own `document` field no longer matches the file it
    // sits beside -- as if it had been copied from a different document's
    // manifest by hand.
    let manifest_path = dir.path().join("tenants.manifest.json");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    std::fs::write(&manifest_path, manifest.replace("\"tenants\"", "\"catalog\"")).unwrap();

    let error = adapter(&dir).current().await.unwrap_err();

    assert!(matches!(error, PublicationError::Unreadable { .. }));
}

#[tokio::test]
async fn current_reports_the_held_manifests_revision_regardless_of_payload_presence() {
    let dir = TempDir::new("current-presence-states");
    let publisher = adapter(&dir);

    // No manifest published yet: every document reports unpublished.
    let unpublished = publisher.current().await.unwrap();
    assert_eq!(unpublished.tenants, None);
    assert_eq!(unpublished.data_sources, None);
    assert_eq!(unpublished.catalog, None);

    publisher.publish(&snapshot(3, "acme", "sql-01")).await.unwrap();

    // Manifest and payload both held: current() reports the published
    // revision.
    let held = publisher.current().await.unwrap();
    assert_eq!(held.tenants, Some(DocumentRevision::new(3)));
    assert_eq!(held.data_sources, Some(DocumentRevision::new(3)));
    assert_eq!(held.catalog, Some(DocumentRevision::new(3)));

    // Manifest held, payload lost: current() still reports the manifest's
    // own revision -- it never reads the payload.
    std::fs::remove_file(dir.path().join("tenants.json")).unwrap();
    let payload_lost = publisher.current().await.unwrap();
    assert_eq!(payload_lost.tenants, Some(DocumentRevision::new(3)));
}

#[tokio::test]
async fn a_payload_without_a_manifest_is_treated_as_a_first_publication() {
    let dir = TempDir::new("orphaned-payload");
    // Simulates the shipped `examples/*.json`: a valid payload with no
    // manifest beside it, naming a *different* DataSource than the one
    // about to be published -- proving the divergence guard is off in this
    // state, not merely that malformed bytes get overwritten.
    let orphaned =
        fabric_runtime_publication::data_sources_canonical_json(&[data_source("sql-99", 7)]).unwrap();
    std::fs::write(dir.path().join("data-sources.json"), orphaned).unwrap();

    let report = adapter(&dir)
        .publish(&snapshot(5, "acme", "sql-01"))
        .await
        .unwrap();

    assert_eq!(report.data_sources, DocumentOutcome::Written);
    let written = std::fs::read_to_string(dir.path().join("data-sources.json")).unwrap();
    assert!(
        written.contains("sql-01") && !written.contains("sql-99"),
        "{written}"
    );
}

/// The fix for the tenants half of the orphaned-payload state above: a
/// populated tenants payload with no manifest beside it is held content,
/// not an assumed-empty set, so its own emptying guard still fires. Before
/// the fix, `held_tenants.len()` read as zero here and an offered
/// `tenants: []` at `Emptying::NotIntended` would have been accepted and
/// written straight over a populated file.
#[tokio::test]
async fn a_populated_tenants_payload_with_no_manifest_still_guards_against_emptying() {
    let dir = TempDir::new("orphaned-tenants-payload-emptying");

    // Seeded directly, the way the shipped `examples/*.json` ship today: a
    // populated tenants payload, no manifest beside it.
    let seeded_tenants = vec![tenant("acme", "sql-01", 1)];
    let seeded_bytes = fabric_runtime_publication::tenants_canonical_json(&seeded_tenants).unwrap();
    std::fs::write(dir.path().join("tenants.json"), &seeded_bytes).unwrap();

    let names = [
        "tenants.json",
        "tenants.manifest.json",
        "data-sources.json",
        "data-sources.manifest.json",
        "catalog.json",
        "catalog.manifest.json",
    ];
    let before: Vec<Option<(u64, i64)>> = names
        .iter()
        .map(|name| existing_identity(&dir.path().join(name)))
        .collect();

    // Data sources still names "sql-01" so the retirement guard has nothing
    // to say -- the only guard that can fire here is the tenants document's
    // own emptying guard.
    let refused = RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(1), vec![]),
        data_sources: DocumentInput::new(DocumentRevision::new(1), vec![data_source("sql-01", 1)]),
        catalog: DocumentInput::new(DocumentRevision::new(1), catalog()),
    };

    let error = adapter(&dir).publish(&refused).await.unwrap_err();

    assert!(matches!(
        error,
        PublicationError::EmptyingNotIntended {
            document: fabric_runtime_publication::DocumentKind::Tenants
        }
    ));

    let after: Vec<Option<(u64, i64)>> = names
        .iter()
        .map(|name| existing_identity(&dir.path().join(name)))
        .collect();
    assert_eq!(before, after);
    assert_eq!(
        std::fs::read(dir.path().join("tenants.json")).unwrap(),
        seeded_bytes
    );
}

/// A directory sitting at the exact sibling path `atomic_write` stages its
/// temporary file under, so `File::create` fails for that one write while
/// leaving the *real* target path (read at the start of every `publish`
/// call) untouched -- unlike putting the directory at the manifest's own
/// path, which would make reading the held manifest fail before any write
/// was even attempted.
fn obstruct_temp_file_for(dir: &TempDir, file_name: &str) {
    std::fs::create_dir(dir.path().join(format!(".{file_name}.tmp"))).unwrap();
}

#[tokio::test]
async fn the_manifest_is_written_after_the_payload_it_describes() {
    let dir = TempDir::new("manifest-after-payload");
    obstruct_temp_file_for(&dir, "data-sources.manifest.json");

    let error = adapter(&dir)
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap_err();

    assert!(matches!(error, PublicationError::Unwritable { .. }));
    let payload = std::fs::read_to_string(dir.path().join("data-sources.json")).unwrap();
    assert!(payload.contains("sql-01"), "{payload}");
    assert!(!dir.path().join("data-sources.manifest.json").exists());
}

/// B2: an I/O failure between two documents' writes is not a refusal --
/// `errors.rs` and `report.rs` are explicit that this is the one case where
/// earlier documents stay written. Obstruct the catalogue's own payload
/// write so data sources land and the catalogue fails before tenants is
/// even attempted, then republish unobstructed at the same revisions and
/// confirm the second call finishes what the first left half-done.
#[tokio::test]
async fn a_publication_that_failed_between_documents_is_completed_by_the_next_one() {
    let dir = TempDir::new("recovers-after-partial-write");
    obstruct_temp_file_for(&dir, "catalog.json");

    let error = adapter(&dir)
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap_err();

    assert!(matches!(error, PublicationError::Unwritable { .. }));
    assert!(dir.path().join("data-sources.json").exists());
    assert!(dir.path().join("data-sources.manifest.json").exists());
    assert!(!dir.path().join("catalog.json").exists());
    assert!(!dir.path().join("tenants.json").exists());

    std::fs::remove_dir(dir.path().join(".catalog.json.tmp")).unwrap();

    let report = adapter(&dir)
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap();

    assert_eq!(report.data_sources, DocumentOutcome::Unchanged);
    assert_eq!(report.catalog, DocumentOutcome::Written);
    assert_eq!(report.tenants, DocumentOutcome::Written);
}

/// Proves temp-file cleanup, not sibling placement or rename-only creation
/// (both already exercised, without a dedicated assertion, by every other
/// test in this file that publishes into a directory and checks what is in
/// it): the sibling temp file used to stage a write is gone afterwards,
/// whether that write succeeded or failed.
#[tokio::test]
async fn the_temp_file_never_survives_a_publish_call_success_or_failure() {
    let dir = TempDir::new("rename-only");

    adapter(&dir)
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap();
    assert!(temp_files_in(dir.path()).is_empty());

    // Force a failure after the payload's rename but before the manifest's,
    // the same way `the_manifest_is_written_after_the_payload_it_describes`
    // does, and check the temp file used to stage that failed write is
    // still gone -- the sibling never survives past this call either way.
    let dir = TempDir::new("rename-only-failure");
    obstruct_temp_file_for(&dir, "data-sources.manifest.json");
    let _ = adapter(&dir).publish(&snapshot(1, "acme", "sql-01")).await;

    // The obstruction itself is a directory named like a temp file; the
    // adapter's own bytes-based scan only counts *files*, so confirm
    // directly that nothing beyond that pre-existing obstruction was left.
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert_eq!(entries.len(), 2, "{entries:?}"); // data-sources.json + the obstruction
}

#[tokio::test]
async fn the_port_describes_itself_without_naming_a_credential() {
    let dir = TempDir::new("describe");
    let description = adapter(&dir).describe();

    assert!(description.contains("tenants.json"), "{description}");
    for word in ["secret", "password", "token", "credential"] {
        assert!(!description.to_lowercase().contains(word), "{description}");
    }
}

#[tokio::test]
async fn the_adapters_output_is_read_by_the_runtimes_own_json_file_source() {
    let dir = TempDir::new("real-consumer");
    adapter(&dir)
        .publish(&snapshot(1, "acme", "sql-01"))
        .await
        .unwrap();

    let tenants = JsonFileSource::<TenantRuntimeBinding>::new(dir.path().join("tenants.json"))
        .load()
        .await
        .unwrap();
    let data_sources = JsonFileSource::<RuntimeDataSource>::new(dir.path().join("data-sources.json"))
        .load()
        .await
        .unwrap();

    assert_eq!(tenants.len(), 1);
    assert_eq!(data_sources.len(), 1);
}
