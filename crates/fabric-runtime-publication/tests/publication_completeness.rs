//! G3's panel state, at the adapter: `inspect` reads the held record and
//! every part it names, and says whether the filesystem holds that whole
//! generation. Revisions alone cannot say so: `current()` falls back to the
//! record when a manifest is lost, and reports nothing about digests.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_core::TenantId;
use fabric_runtime_publication::{
    CompletenessGap, DocumentKind, DocumentRevision, GapKind, IssuerRegistration, PublicationCompleteness,
    PublicationError, RecordedPart, RegistryIssuer, RuntimePublication as _, RuntimeSnapshot,
    UnobservedReason, CATALOG_FILE, DATA_SOURCES_FILE, DATA_SOURCES_MANIFEST_FILE, ISSUER_REGISTRY_FILE,
    PUBLICATION_RECORD_FILE, TENANTS_FILE,
};
use support::TempDir;

const BLOCK_TENANTS: &str = ".tenants.json.tmp";

fn gap(part: RecordedPart, kind: GapKind) -> CompletenessGap {
    CompletenessGap { part, kind }
}

fn gaps(completeness: PublicationCompleteness) -> Vec<CompletenessGap> {
    match completeness {
        PublicationCompleteness::Incomplete { gaps, .. } => gaps,
        other => panic!("expected incomplete, got {other:?}"),
    }
}

/// Every document at revision 2. The catalogue's bytes do not change, so an
/// interrupted write leaves the data sources ahead of the record in revision
/// and digest, and the catalogue in revision only.
fn next() -> RuntimeSnapshot {
    support::base_snapshot(2)
}

async fn published() -> TempDir {
    let dir = TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    dir
}

#[tokio::test]
async fn nothing_published_is_unpublished_and_a_publication_is_complete_at_the_record_it_stamped() {
    let dir = TempDir::new();
    assert_eq!(
        dir.publisher().inspect().await,
        PublicationCompleteness::Unpublished
    );

    let report = dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();

    assert_eq!(
        dir.publisher().inspect().await,
        PublicationCompleteness::Complete {
            record: report.record
        }
    );
}

#[tokio::test]
async fn documents_without_a_record_are_unpublished() {
    let dir = published().await;
    dir.remove_raw(PUBLICATION_RECORD_FILE);

    assert_eq!(
        dir.publisher().inspect().await,
        PublicationCompleteness::Unpublished
    );
}

#[tokio::test]
async fn a_publication_interrupted_part_way_is_incomplete_until_it_is_completed() {
    let dir = published().await;
    let held = dir.publisher().inspect().await;
    std::fs::create_dir(dir.path().join(BLOCK_TENANTS)).unwrap();

    let error = dir.publisher().publish(&next()).await.unwrap_err();
    assert!(matches!(
        error,
        PublicationError::Unwritable {
            document: DocumentKind::Tenants,
            ..
        }
    ));

    let PublicationCompleteness::Complete { record } = held else {
        panic!("the base generation must be complete: {held:?}");
    };
    let interrupted = dir.publisher().inspect().await;
    assert!(
        matches!(&interrupted, PublicationCompleteness::Incomplete { record: at, .. } if *at == record),
        "the record still names the base generation: {interrupted:?}"
    );
    assert_eq!(
        gaps(interrupted),
        vec![
            gap(RecordedPart::DataSources, GapKind::RevisionMismatch),
            gap(RecordedPart::DataSources, GapKind::DigestMismatch),
            gap(RecordedPart::Catalog, GapKind::RevisionMismatch),
        ]
    );

    std::fs::remove_dir(dir.path().join(BLOCK_TENANTS)).unwrap();
    let report = dir.publisher().publish(&next()).await.unwrap();
    assert_eq!(
        dir.publisher().inspect().await,
        PublicationCompleteness::Complete {
            record: report.record
        }
    );
}

#[tokio::test]
async fn a_lost_payload_is_incomplete_though_its_manifest_still_reports_the_revision() {
    let dir = published().await;
    dir.remove_raw(DATA_SOURCES_FILE);

    assert_eq!(
        dir.publisher().current().await.unwrap().data_sources,
        Some(DocumentRevision::new(1))
    );
    assert_eq!(
        gaps(dir.publisher().inspect().await),
        vec![gap(RecordedPart::DataSources, GapKind::PayloadMissing)]
    );
}

#[tokio::test]
async fn a_lost_manifest_is_incomplete_though_current_falls_back_to_the_records_revision() {
    let dir = published().await;
    dir.remove_raw(DATA_SOURCES_MANIFEST_FILE);

    assert_eq!(
        dir.publisher().current().await.unwrap().data_sources,
        Some(DocumentRevision::new(1)),
        "current() reads the record's revision in place of the lost manifest"
    );
    assert_eq!(
        gaps(dir.publisher().inspect().await),
        vec![gap(RecordedPart::DataSources, GapKind::ManifestMissing)]
    );
}

#[tokio::test]
async fn equal_revisions_with_other_bytes_are_incomplete() {
    let dir = published().await;
    let before = dir.publisher().current().await.unwrap();
    dir.write_raw(CATALOG_FILE, b"{}");

    assert_eq!(dir.publisher().current().await.unwrap(), before);
    assert_eq!(
        gaps(dir.publisher().inspect().await),
        vec![gap(RecordedPart::Catalog, GapKind::DigestMismatch)]
    );
}

#[tokio::test]
async fn an_issuer_registry_the_record_names_is_held_to_its_digest() {
    let dir = TempDir::new();
    let snapshot = RuntimeSnapshot {
        registry: Some(vec![IssuerRegistration {
            issuer: RegistryIssuer::parse(&support::issuer_for("acme")).unwrap(),
            tenant: TenantId::try_new("acme").unwrap(),
        }]),
        ..support::base_snapshot(1)
    };
    let report = dir.publisher().publish(&snapshot).await.unwrap();
    assert_eq!(
        dir.publisher().inspect().await,
        PublicationCompleteness::Complete {
            record: report.record
        }
    );

    let registry = dir.read_raw(ISSUER_REGISTRY_FILE);
    dir.write_raw(ISSUER_REGISTRY_FILE, b"{\"generation\":1,\"issuers\":[]}");
    assert_eq!(
        gaps(dir.publisher().inspect().await),
        vec![gap(RecordedPart::IssuerRegistry, GapKind::DigestMismatch)]
    );

    dir.remove_raw(ISSUER_REGISTRY_FILE);
    assert_eq!(
        gaps(dir.publisher().inspect().await),
        vec![gap(RecordedPart::IssuerRegistry, GapKind::PayloadMissing)]
    );

    dir.write_raw(ISSUER_REGISTRY_FILE, &registry);
    assert!(matches!(
        dir.publisher().inspect().await,
        PublicationCompleteness::Complete { .. }
    ));
}

#[tokio::test]
async fn a_part_that_cannot_be_read_leaves_the_target_unobserved() {
    let dir = published().await;
    dir.remove_raw(TENANTS_FILE);
    std::fs::create_dir(dir.path().join(TENANTS_FILE)).unwrap();

    assert_eq!(
        dir.publisher().inspect().await,
        PublicationCompleteness::Unobserved {
            reason: UnobservedReason::Unavailable
        }
    );
}

#[tokio::test]
async fn a_record_this_publisher_cannot_read_leaves_the_target_unobserved() {
    let dir = published().await;
    dir.write_raw(PUBLICATION_RECORD_FILE, b"not a record");

    assert_eq!(
        dir.publisher().inspect().await,
        PublicationCompleteness::Unobserved {
            reason: UnobservedReason::RecordUnreadable
        }
    );
}
