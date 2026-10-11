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
