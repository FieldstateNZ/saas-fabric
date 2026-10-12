//! A restore puts back only the bytes the held record names, at the
//! record's revision, and only for a document that blocks publication.

use super::*;
use crate::{ObservedDocument, RecordedDocument, RecordedDocuments, ResourceHistory, CONTRACT_VERSION};

const GOOD: &[u8] = b"[]";
const TORN: &[u8] = b"[{\"tenant\":";

fn manifest(document: DocumentKind, revision: u64) -> Vec<u8> {
    DocumentManifest::new(document, DocumentRevision::new(revision))
        .canonical_json()
        .unwrap()
}

fn record(revision: u64) -> Vec<u8> {
    let recorded = RecordedDocument::of(DocumentRevision::new(revision), GOOD);
    PublicationRecord::new(
        7,
        RecordedDocuments {
            tenants: recorded.clone(),
            data_sources: recorded.clone(),
            catalog: recorded,
        },
        ResourceHistory::default(),
    )
    .canonical_json()
    .unwrap()
}

/// Both documents held at revision 3 under a record naming `GOOD` for each,
/// with the tenants payload replaced by `tenants`.
fn held(tenants: Option<&[u8]>) -> TargetObservation {
    let document = |kind, payload: Option<&[u8]>| ObservedDocument {
        manifest: Some(manifest(kind, 3)),
        payload: payload.map(<[u8]>::to_vec),
    };
    TargetObservation {
        record: Some(record(3)),
        tenants: document(DocumentKind::Tenants, tenants),
        data_sources: document(DocumentKind::DataSources, Some(GOOD)),
        catalog: document(DocumentKind::Catalog, Some(GOOD)),
        registry: None,
    }
}

#[test]
fn an_unparseable_payload_is_restored_from_the_recorded_bytes_at_the_recorded_revision() {
    let plan = plan_restore(&held(Some(TORN)), DocumentKind::Tenants, GOOD).unwrap();

    assert_eq!(plan.revision, DocumentRevision::new(3));
    assert_eq!(plan.generation, 7);
    assert_eq!(plan.payload.as_deref(), Some(GOOD));
    assert_eq!(plan.manifest, None, "the held manifest already states revision 3");
    assert_eq!(plan.held_payload_sha256, Some(sha256_hex(TORN)));
    assert!(plan.blocked_because.starts_with("unreadable payload"));
}

#[test]
fn a_lost_payload_is_restored_under_its_held_manifest() {
    let plan = plan_restore(&held(None), DocumentKind::Tenants, GOOD).unwrap();

    assert_eq!(plan.payload.as_deref(), Some(GOOD));
    assert_eq!(plan.manifest, None);
    assert!(plan.blocked_because.starts_with("held payload lost"));
}

#[test]
fn an_unreadable_manifest_is_rewritten_at_the_records_revision_never_another() {
    let mut observation = held(Some(GOOD));
    observation.data_sources.manifest = Some(b"{not a manifest".to_vec());
    observation.data_sources.payload = Some(GOOD.to_vec());

    let plan = plan_restore(&observation, DocumentKind::DataSources, GOOD).unwrap();

    assert_eq!(
        plan.payload, None,
        "the held payload is already the recorded bytes"
    );
    assert_eq!(
        plan.manifest,
        Some(DocumentManifest::new(
            DocumentKind::DataSources,
            DocumentRevision::new(3)
        ))
    );
}

#[test]
fn a_manifest_naming_another_document_is_unreadable_and_rewritten() {
    let mut observation = held(Some(GOOD));
    observation.tenants.manifest = Some(manifest(DocumentKind::DataSources, 3));

    let plan = plan_restore(&observation, DocumentKind::Tenants, GOOD).unwrap();

    assert_eq!(
        plan.manifest.map(|manifest| manifest.document()),
        Some(DocumentKind::Tenants)
    );
}

#[test]
fn bytes_the_record_does_not_name_are_refused_even_when_they_parse() {
    let error = plan_restore(&held(Some(TORN)), DocumentKind::Tenants, b"[ ]").unwrap_err();

    assert_eq!(
        error,
        RestoreRefusal::DigestMismatch {
            document: DocumentKind::Tenants,
            recorded: sha256_hex(GOOD),
            offered: sha256_hex(b"[ ]"),
        }
    );
}

#[test]
fn a_document_that_does_not_block_publication_is_left_alone() {
    let error = plan_restore(&held(Some(GOOD)), DocumentKind::Tenants, GOOD).unwrap_err();

    assert_eq!(
        error,
        RestoreRefusal::NotBlocked {
            document: DocumentKind::Tenants
        }
    );
}

#[test]
fn the_catalogue_is_never_restored() {
    let error = plan_restore(&held(Some(TORN)), DocumentKind::Catalog, GOOD).unwrap_err();

    assert_eq!(
        error,
        RestoreRefusal::NotRestorable {
            document: DocumentKind::Catalog
        }
    );
}

#[test]
fn with_no_record_nothing_vouches_for_the_bytes() {
    let mut observation = held(Some(TORN));
    observation.record = None;

    assert_eq!(
        plan_restore(&observation, DocumentKind::Tenants, GOOD).unwrap_err(),
        RestoreRefusal::NoRecord
    );
}

#[test]
fn an_unreadable_record_or_one_above_this_publishers_floor_is_refused() {
    let mut unreadable = held(Some(TORN));
    unreadable.record = Some(b"{".to_vec());
    assert!(matches!(
        plan_restore(&unreadable, DocumentKind::Tenants, GOOD).unwrap_err(),
        RestoreRefusal::RecordUnusable { .. }
    ));

    let mut above = held(Some(TORN));
    let mut record: serde_json::Value = serde_json::from_slice(&record(3)).unwrap();
    record["minimum_publisher_contract"] = (CONTRACT_VERSION + 1).into();
    above.record = Some(serde_json::to_vec(&record).unwrap());
    let error = plan_restore(&above, DocumentKind::Tenants, GOOD).unwrap_err();
    assert!(
        matches!(&error, RestoreRefusal::RecordUnusable { detail } if detail.contains("publisher contract")),
        "{error:?}"
    );
}

#[test]
fn a_manifest_ahead_of_the_record_is_refused_so_no_revision_goes_back() {
    // An interrupted publication: the manifest reached revision 4, the
    // record still names 3. Putting revision 3's bytes under it would hold
    // content its manifest does not describe.
    let mut observation = held(Some(TORN));
    observation.tenants.manifest = Some(manifest(DocumentKind::Tenants, 4));

    assert_eq!(
        plan_restore(&observation, DocumentKind::Tenants, GOOD).unwrap_err(),
        RestoreRefusal::ManifestDisagrees {
            document: DocumentKind::Tenants,
            manifest: DocumentRevision::new(4),
            recorded: DocumentRevision::new(3),
        }
    );
}

#[test]
fn a_manifest_behind_the_record_is_refused_too() {
    let mut observation = held(Some(TORN));
    observation.tenants.manifest = Some(manifest(DocumentKind::Tenants, 2));

    assert!(matches!(
        plan_restore(&observation, DocumentKind::Tenants, GOOD).unwrap_err(),
        RestoreRefusal::ManifestDisagrees { .. }
    ));
}

#[test]
fn recorded_bytes_that_do_not_parse_are_refused() {
    let mut observation = held(Some(TORN));
    let recorded = RecordedDocument::of(DocumentRevision::new(3), b"{}");
    observation.record = Some(
        PublicationRecord::new(
            7,
            RecordedDocuments {
                tenants: recorded.clone(),
                data_sources: recorded.clone(),
                catalog: recorded,
            },
            ResourceHistory::default(),
        )
        .canonical_json()
        .unwrap(),
    );

    assert!(matches!(
        plan_restore(&observation, DocumentKind::Tenants, b"{}").unwrap_err(),
        RestoreRefusal::OfferedUnparseable { .. }
    ));
}

#[test]
fn the_plan_reports_what_it_writes_and_never_the_bytes() {
    let plan = plan_restore(&held(None), DocumentKind::Tenants, GOOD).unwrap();
    let report = serde_json::to_value(&plan).unwrap();

    assert_eq!(report["writes_payload"], true);
    assert_eq!(report["writes_manifest"], serde_json::Value::Null);
    assert_eq!(report["revision"], 3);
    assert_eq!(report["document"], "tenants");
}
