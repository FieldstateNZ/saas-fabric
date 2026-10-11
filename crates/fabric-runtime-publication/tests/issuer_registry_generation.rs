//! The issuer registry generation (D01-4) through the real
//! `FilesystemRuntimePublication`: deterministic, stamped with a generation
//! that only moves forward, held to the tenants document of the same
//! publication, and -- for ADR 0019 G4a -- rendered into the runtime's
//! `[identity].trusted_issuers` without translation, so the issuer the edge
//! admits and the tenant the runtime binds it to are one fact.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::time::Duration;

use fabric_core::TenantId;
use fabric_runtime_publication::{
    DocumentOutcome, IssuerRegistration, PublicationError, PublicationRecord, RecordedDocument,
    RegistryGeneration, RegistryIssuer, RegistryRefusal, RuntimePublication as _, RuntimeSnapshot,
    ISSUER_REGISTRY_FILE, PUBLICATION_RECORD_FILE,
};
use http::StatusCode;
use support::{Reader, TempDir};
use tower::ServiceExt as _;

fn registered(tenant: &str) -> IssuerRegistration {
    IssuerRegistration {
        issuer: RegistryIssuer::parse(&support::issuer_for(tenant)).unwrap(),
        tenant: TenantId::try_new(tenant).unwrap(),
    }
}

/// The fixture's two tenants and documents at `revision`, carrying a
/// registry that registers `tenants`.
fn snapshot(revision: u64, tenants: &[&str]) -> RuntimeSnapshot {
    RuntimeSnapshot {
        registry: Some(tenants.iter().copied().map(registered).collect()),
        ..support::base_snapshot(revision)
    }
}

fn record(dir: &TempDir) -> PublicationRecord {
    PublicationRecord::parse_held(&dir.read_raw(PUBLICATION_RECORD_FILE)).unwrap()
}

fn registry_generation(dir: &TempDir) -> u64 {
    let registry: serde_json::Value = serde_json::from_slice(&dir.read_raw(ISSUER_REGISTRY_FILE)).unwrap();
    registry["generation"].as_u64().unwrap()
}

#[tokio::test]
async fn the_same_registrations_publish_the_same_bytes_whatever_order_they_are_offered_in() {
    let forward = TempDir::new();
    let reversed = TempDir::new();

    forward
        .publisher()
        .publish(&snapshot(1, &["acme", "globex"]))
        .await
        .unwrap();
    reversed
        .publisher()
        .publish(&snapshot(1, &["globex", "acme"]))
        .await
        .unwrap();

    assert_eq!(
        forward.read_raw(ISSUER_REGISTRY_FILE),
        reversed.read_raw(ISSUER_REGISTRY_FILE)
    );
    assert_eq!(
        forward.read_raw(PUBLICATION_RECORD_FILE),
        reversed.read_raw(PUBLICATION_RECORD_FILE)
    );
}

#[tokio::test]
async fn the_record_names_the_registry_generation_and_the_digest_of_its_exact_bytes() {
    let dir = TempDir::new();

    let report = dir.publisher().publish(&snapshot(1, &["acme"])).await.unwrap();

    let registry = report.registry.unwrap();
    assert_eq!(registry.outcome, DocumentOutcome::Written);
    assert_eq!(registry.generation, RegistryGeneration::FIRST);
    let recorded = record(&dir).registry().cloned().unwrap();
    assert_eq!(recorded.generation, RegistryGeneration::FIRST);
    assert_eq!(
        recorded.sha256,
        RecordedDocument::of(
            fabric_runtime_publication::DocumentRevision::new(1),
            &dir.read_raw(ISSUER_REGISTRY_FILE)
        )
        .sha256
    );
    assert_eq!(
        dir.publisher().current().await.unwrap().registry,
        Some(RegistryGeneration::FIRST)
    );
}

#[tokio::test]
async fn a_publication_without_a_registry_writes_no_registry_and_records_none() {
    let dir = TempDir::new();

    let report = dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();

    assert_eq!(report.registry, None);
    assert!(!dir.has(ISSUER_REGISTRY_FILE));
    assert!(record(&dir).registry().is_none());
    let text = String::from_utf8(dir.read_raw(PUBLICATION_RECORD_FILE)).unwrap();
    assert!(!text.contains("registry"), "{text}");
}

#[tokio::test]
async fn unchanged_registrations_keep_their_generation_while_the_publication_record_advances() {
    let dir = TempDir::new();
    dir.publisher().publish(&snapshot(1, &["acme"])).await.unwrap();
    let before = dir.read_raw(ISSUER_REGISTRY_FILE);

    let report = dir.publisher().publish(&snapshot(2, &["acme"])).await.unwrap();

    assert_eq!(report.tenants, DocumentOutcome::Written);
    assert_eq!(report.registry.unwrap().outcome, DocumentOutcome::Unchanged);
    assert_eq!(dir.read_raw(ISSUER_REGISTRY_FILE), before);
    assert_eq!(record(&dir).generation(), 2);
    assert_eq!(
        record(&dir).registry().unwrap().generation,
        RegistryGeneration::FIRST
    );
}

#[tokio::test]
async fn each_change_to_the_registrations_advances_the_generation_by_exactly_one() {
    let dir = TempDir::new();
    let publisher = dir.publisher();
    let mut seen = Vec::new();

    for tenants in [
        &["acme"][..],
        &["acme"],
        &["acme", "globex"],
        &["acme"],
        &[],
        &[],
        &["globex"],
    ] {
        let report = publisher.publish(&snapshot(1, tenants)).await.unwrap();
        assert_eq!(
            report.registry.unwrap().generation.get(),
            registry_generation(&dir)
        );
        seen.push(registry_generation(&dir));
    }

    assert_eq!(seen, vec![1, 1, 2, 3, 4, 4, 5]);
    assert_eq!(record(&dir).registry().unwrap().generation.get(), 5);
}

#[tokio::test]
async fn a_registration_naming_a_tenant_the_tenants_document_does_not_bind_is_refused_before_any_write() {
    let dir = TempDir::new();
    dir.publisher().publish(&snapshot(1, &["acme"])).await.unwrap();
    let before = dir.capture(&[ISSUER_REGISTRY_FILE, PUBLICATION_RECORD_FILE, "tenants.json"]);

    let error = dir
        .publisher()
        .publish(&snapshot(1, &["acme", "initech"]))
        .await
        .unwrap_err();

    assert!(
        matches!(
            &error,
            PublicationError::Registry(RegistryRefusal::TenantUnbound { tenant, .. }) if tenant.as_str() == "initech"
        ),
        "{error:?}"
    );
    assert_eq!(
        dir.capture(&[ISSUER_REGISTRY_FILE, PUBLICATION_RECORD_FILE, "tenants.json"]),
        before
    );
}

#[tokio::test]
async fn a_published_registry_is_never_dropped_only_replaced_by_a_later_generation() {
    let dir = TempDir::new();
    dir.publisher().publish(&snapshot(1, &["acme"])).await.unwrap();
    let before = dir.capture(&[ISSUER_REGISTRY_FILE, PUBLICATION_RECORD_FILE]);

    let error = dir
        .publisher()
        .publish(&support::base_snapshot(2))
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            PublicationError::Registry(RegistryRefusal::Withdrawn { held }) if held == RegistryGeneration::FIRST
        ),
        "{error:?}"
    );
    assert_eq!(
        dir.capture(&[ISSUER_REGISTRY_FILE, PUBLICATION_RECORD_FILE]),
        before
    );
}

#[tokio::test]
async fn a_publication_interrupted_before_its_record_is_finished_at_the_next_generation_and_never_reuses_one()
{
    let dir = TempDir::new();
    dir.publisher().publish(&snapshot(1, &["acme"])).await.unwrap();
    let first_record = dir.capture(&[PUBLICATION_RECORD_FILE]);
    dir.publisher()
        .publish(&snapshot(1, &["acme", "globex"]))
        .await
        .unwrap();
    // The registry landed and the record did not: readers still hold
    // generation 1, and the bytes on disk are an unrecorded generation 2.
    dir.restore(&first_record);

    let report = dir
        .publisher()
        .publish(&snapshot(1, &["acme", "globex"]))
        .await
        .unwrap();

    assert_eq!(report.registry.unwrap().generation.get(), 2);
    assert_eq!(record(&dir).registry().unwrap().generation.get(), 2);
    assert_eq!(registry_generation(&dir), 2);
}

#[tokio::test]
async fn a_contract_one_reader_serves_a_generation_whose_record_names_a_registry() {
    let stack = support::build_stack_with(&snapshot(1, &["acme", "globex"]), Reader::Generation).await;

    let response = stack
        .app
        .clone()
        .oneshot(support::request("GET", "/articles", &support::claims_for("acme")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// ADR 0019 G4a: the gateway's allow-list and the runtime's
/// `[identity].trusted_issuers` come from one generator. The registry's
/// `issuers` are `fabric_identity::TrustedIssuer`s field for field, so the
/// runtime trusts exactly what the registry registers: each issuer reaches
/// its own tenant, and a tenant whose binding is published but whose client
/// is not registered -- not ready, or tombstoned -- is reached by no token.
#[tokio::test]
async fn the_registry_is_the_runtimes_trusted_issuers_and_routes_each_issuer_to_its_tenant_and_no_other() {
    let dir = TempDir::new();
    dir.publisher().publish(&snapshot(1, &["acme"])).await.unwrap();
    let registry: serde_json::Value = serde_json::from_slice(&dir.read_raw(ISSUER_REGISTRY_FILE)).unwrap();
    let trusted: Vec<fabric_identity::TrustedIssuer> =
        serde_json::from_value(registry["issuers"].clone()).unwrap();
    fabric_identity::TrustedIssuer::validate_registry(&trusted).unwrap();

    let stack = support::assemble(dir, Reader::Generation, trusted).await;

    let acme = stack
        .app
        .clone()
        .oneshot(support::request("GET", "/articles", &support::claims_for("acme")))
        .await
        .unwrap();
    assert_eq!(acme.status(), StatusCode::OK);
    let rows = support::body_json(acme).await;
    assert_eq!(rows["data"][0]["title"], "Acme Handbook", "{rows}");
    assert_eq!(rows["data"].as_array().map(Vec::len), Some(1), "{rows}");

    let globex = support::poll_for_status(
        &stack.app,
        StatusCode::UNAUTHORIZED,
        || support::request("GET", "/articles", &support::claims_for("globex")),
        Duration::from_secs(1),
    )
    .await;
    assert_eq!(globex.status(), StatusCode::UNAUTHORIZED);
}

/// The changed-offer recovery: generation 2 reached the disk without its
/// record, and the next offer differs from it. Generation 2 is not
/// reissued for those other registrations, and going back to the committed
/// set does not hide it either.
#[tokio::test]
async fn a_changed_offer_after_an_interrupted_publication_never_reuses_the_uncommitted_generation() {
    for recovery in [&["globex"][..], &["acme"]] {
        let dir = TempDir::new();
        dir.publisher().publish(&snapshot(1, &["acme"])).await.unwrap();
        let committed = dir.capture(&[PUBLICATION_RECORD_FILE]);
        dir.publisher()
            .publish(&snapshot(1, &["acme", "globex"]))
            .await
            .unwrap();
        let uncommitted = dir.read_raw(ISSUER_REGISTRY_FILE);
        dir.restore(&committed);

        let report = dir.publisher().publish(&snapshot(1, recovery)).await.unwrap();

        assert_eq!(report.registry.unwrap().generation.get(), 3, "{recovery:?}");
        assert_eq!(registry_generation(&dir), 3, "{recovery:?}");
        assert_eq!(
            record(&dir).registry().unwrap().generation.get(),
            3,
            "{recovery:?}"
        );
        assert_ne!(dir.read_raw(ISSUER_REGISTRY_FILE), uncommitted);
    }
}

#[tokio::test]
async fn held_registry_bytes_that_do_not_parse_refuse_the_publication_and_write_nothing() {
    let dir = TempDir::new();
    dir.publisher().publish(&snapshot(1, &["acme"])).await.unwrap();
    dir.write_raw(ISSUER_REGISTRY_FILE, b"{truncated");
    let before = dir.capture(&[ISSUER_REGISTRY_FILE, PUBLICATION_RECORD_FILE]);

    let error = dir
        .publisher()
        .publish(&snapshot(1, &["acme", "globex"]))
        .await
        .unwrap_err();

    assert!(matches!(error, PublicationError::Unreadable { .. }), "{error:?}");
    assert_eq!(
        dir.capture(&[ISSUER_REGISTRY_FILE, PUBLICATION_RECORD_FILE]),
        before
    );
}

const DOCUMENT_FILES: [&str; 6] = [
    "tenants.json",
    "tenants.manifest.json",
    "data-sources.json",
    "data-sources.manifest.json",
    "catalog.json",
    "catalog.manifest.json",
];

/// Rewrites what `dir` holds as #139's contract-2 publisher left it: every
/// manifest at contract 2, and a contract-2 record with its floor at 2,
/// resource history and no registry.
fn as_left_by_contract_two(dir: &TempDir) {
    for manifest in [
        "tenants.manifest.json",
        "data-sources.manifest.json",
        "catalog.manifest.json",
    ] {
        let mut value: serde_json::Value = serde_json::from_slice(&dir.read_raw(manifest)).unwrap();
        value["contract_version"] = serde_json::json!(2);
        let mut bytes = serde_json::to_vec_pretty(&value).unwrap();
        bytes.push(b'\n');
        dir.write_raw(manifest, &bytes);
    }
    let mut record: serde_json::Value =
        serde_json::from_slice(&dir.read_raw(PUBLICATION_RECORD_FILE)).unwrap();
    record["contract_version"] = serde_json::json!(2);
    record["minimum_publisher_contract"] = serde_json::json!(2);
    assert!(record.get("registry").is_none());
    dir.write_raw(
        PUBLICATION_RECORD_FILE,
        &serde_json::to_vec_pretty(&record).unwrap(),
    );
}

/// The contract 2 → 3 migration through the real filesystem publisher: the
/// payloads and the contract-2 manifests are left byte for byte, the
/// record's resource history survives, the floor rises to 3, the registry
/// is added at generation 1, and the contract-1 reader serves the result.
#[tokio::test]
async fn a_contract_two_publication_migrates_to_three_keeping_payloads_manifests_and_history() {
    let dir = TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    as_left_by_contract_two(&dir);
    let before = dir.capture(&DOCUMENT_FILES);

    let report = dir.publisher().publish(&snapshot(1, &["acme"])).await.unwrap();

    assert_eq!(report.tenants, DocumentOutcome::Unchanged);
    assert_eq!(report.data_sources, DocumentOutcome::Unchanged);
    assert_eq!(report.catalog, DocumentOutcome::Unchanged);
    assert_eq!(
        dir.capture(&DOCUMENT_FILES),
        before,
        "payloads and contract-2 manifests untouched"
    );
    let migrated = record(&dir);
    assert_eq!(
        migrated.minimum_publisher_contract(),
        fabric_runtime_publication::CONTRACT_VERSION
    );
    assert_eq!(migrated.minimum_reader_contract(), 1);
    assert_eq!(migrated.registry().unwrap().generation, RegistryGeneration::FIRST);
    let acme = TenantId::try_new("acme").unwrap();
    assert!(
        migrated.resources().tenant(&acme).is_some(),
        "resource history survives"
    );

    let stack = support::assemble(dir, Reader::Generation, support::trusted_issuers()).await;
    let response = stack
        .app
        .clone()
        .oneshot(support::request("GET", "/articles", &support::claims_for("acme")))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a contract-1 reader serves the migrated generation"
    );
}

/// An older writer meeting a record above its floor -- here, a record that
/// carries a registry no publisher can parse -- refuses it by the floor
/// before reading the registry, and writes nothing. This is the check every
/// publisher since #136 runs first, so a contract-2 writer meets a
/// contract-3 record the same way.
#[tokio::test]
async fn a_writer_below_a_registry_records_floor_refuses_it_before_reading_the_registry_and_changes_nothing()
{
    let dir = TempDir::new();
    dir.publisher().publish(&snapshot(1, &["acme"])).await.unwrap();
    let mut later: serde_json::Value =
        serde_json::from_slice(&dir.read_raw(PUBLICATION_RECORD_FILE)).unwrap();
    let floor = fabric_runtime_publication::CONTRACT_VERSION + 1;
    later["minimum_publisher_contract"] = serde_json::json!(floor);
    later["registry"] = serde_json::json!("a registry shape this writer has never seen");
    dir.write_raw(
        PUBLICATION_RECORD_FILE,
        &serde_json::to_vec_pretty(&later).unwrap(),
    );
    let mut every_file = DOCUMENT_FILES.to_vec();
    every_file.extend([PUBLICATION_RECORD_FILE, ISSUER_REGISTRY_FILE]);
    let before = dir.capture(&every_file);

    let error = dir
        .publisher()
        .publish(&snapshot(1, &["acme", "globex"]))
        .await
        .unwrap_err();

    assert!(
        matches!(error, PublicationError::BelowRollbackFloor { floor: refused, .. } if refused == floor),
        "{error:?}"
    );
    assert_eq!(dir.capture(&every_file), before);
}
