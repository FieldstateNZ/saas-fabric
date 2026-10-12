//! The rule every adapter's inspection is judged by, over bytes alone.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::digest::sha256_digest;
use crate::{
    DocumentKind, DocumentManifest, DocumentRevision, PublicationRecord, RecordedDocument, RecordedDocuments,
    ResourceHistory,
};

fn observed(kind: DocumentKind, payload: &[u8]) -> ObservedDocument {
    ObservedDocument {
        manifest: Some(
            DocumentManifest::new(kind, DocumentRevision::new(1))
                .canonical_json()
                .unwrap(),
        ),
        payload: Some(payload.to_vec()),
    }
}

/// A whole generation 4: three documents at revision 1, and its record.
fn whole() -> TargetObservation {
    let recorded = |bytes: &[u8]| RecordedDocument::of(DocumentRevision::new(1), bytes);
    let record = PublicationRecord::new(
        4,
        RecordedDocuments {
            tenants: recorded(b"[]"),
            data_sources: recorded(b"[1]"),
            catalog: recorded(b"{}"),
        },
        ResourceHistory::default(),
    );
    TargetObservation {
        record: Some(record.canonical_json().unwrap()),
        tenants: observed(DocumentKind::Tenants, b"[]"),
        data_sources: observed(DocumentKind::DataSources, b"[1]"),
        catalog: observed(DocumentKind::Catalog, b"{}"),
        registry: None,
    }
}

fn assess(observation: &TargetObservation) -> PublicationCompleteness {
    observation.assess(observation.record.as_deref())
}

#[test]
fn a_whole_generation_is_complete_at_its_records_generation_and_digest() {
    let observation = whole();
    let record = observation.record.clone().unwrap();

    assert_eq!(
        assess(&observation),
        PublicationCompleteness::Complete {
            record: RecordStamp {
                generation: 4,
                sha256: sha256_digest(&record),
            }
        }
    );
}

#[test]
fn a_record_that_moved_while_the_target_was_read_is_unstable_whatever_the_parts_say() {
    let observation = whole();

    for after in [None, Some(b"{}".as_slice())] {
        assert_eq!(
            observation.assess(after),
            PublicationCompleteness::Unobserved {
                reason: UnobservedReason::Unstable
            }
        );
    }
}

#[test]
fn no_record_is_unpublished_and_a_record_that_does_not_parse_is_unobserved() {
    let mut observation = whole();
    observation.record = None;
    assert_eq!(assess(&observation), PublicationCompleteness::Unpublished);

    observation.record = Some(b"{\"generation\":4}".to_vec());
    assert_eq!(
        assess(&observation),
        PublicationCompleteness::Unobserved {
            reason: UnobservedReason::RecordUnreadable
        }
    );
}

#[test]
fn a_manifest_for_another_document_or_that_does_not_parse_is_unreadable() {
    let mut observation = whole();
    observation.tenants.manifest = observed(DocumentKind::Catalog, b"").manifest;
    observation.catalog.manifest = Some(b"not json".to_vec());

    let PublicationCompleteness::Incomplete { gaps, .. } = assess(&observation) else {
        panic!("expected incomplete");
    };
    assert_eq!(
        gaps,
        vec![
            CompletenessGap {
                part: RecordedPart::Tenants,
                kind: GapKind::ManifestUnreadable
            },
            CompletenessGap {
                part: RecordedPart::Catalog,
                kind: GapKind::ManifestUnreadable
            },
        ]
    );
}

#[test]
fn what_an_operator_reads_names_the_part_and_never_its_bytes() {
    let mut observation = whole();
    observation.data_sources.payload = Some(b"secret-looking-bytes".to_vec());

    let PublicationCompleteness::Incomplete { gaps, .. } = assess(&observation) else {
        panic!("expected incomplete");
    };
    let said: Vec<String> = gaps.iter().map(ToString::to_string).collect();
    assert_eq!(
        said,
        vec!["data-sources payload does not match the record's digest"]
    );
}
