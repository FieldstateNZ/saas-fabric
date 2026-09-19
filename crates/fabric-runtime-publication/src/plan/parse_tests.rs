//! Held bytes become typed documents, or a refusal that names the document.

use super::*;
use crate::{DocumentRevision, TenantBindingDocument};

#[test]
fn an_absent_payload_parses_as_an_empty_set() {
    let parsed: Vec<TenantBindingDocument> = parse_documents(None, DocumentKind::DataSources).unwrap();

    assert!(parsed.is_empty());
}

#[test]
fn unparseable_bytes_are_refused_as_unreadable() {
    let error =
        parse_documents::<TenantBindingDocument>(Some(b"not json"), DocumentKind::DataSources).unwrap_err();

    assert!(matches!(error, PublicationError::Unreadable { .. }));
}

#[test]
fn no_manifest_and_no_payload_is_never_published() {
    let parsed: Vec<TenantBindingDocument> = parse_held_documents(None, None, DocumentKind::Tenants).unwrap();

    assert!(parsed.is_empty());
}

#[test]
fn no_manifest_but_a_present_payload_is_parsed_as_held_content() {
    // The fix this test pins down: a payload with no manifest beside it
    // (the shipped `examples/*.json` shape) is held content, not an
    // assumed-empty set -- `T = u64` here only to prove the bytes are
    // actually parsed rather than discarded, independent of what type
    // of document they represent.
    let parsed: Vec<u64> = parse_held_documents(None, Some(b"[1,2,3]"), DocumentKind::Tenants).unwrap();

    assert_eq!(parsed, vec![1, 2, 3]);
}

#[test]
fn a_manifest_without_a_payload_is_refused_as_held_payload_lost() {
    let manifest = DocumentManifest::new(DocumentKind::Tenants, DocumentRevision::new(1));

    let error = parse_held_documents::<TenantBindingDocument>(Some(&manifest), None, DocumentKind::Tenants)
        .unwrap_err();

    assert!(matches!(
        error,
        PublicationError::HeldPayloadLost {
            document: DocumentKind::Tenants
        }
    ));
}

#[test]
fn a_manifest_with_a_payload_parses_it() {
    let manifest = DocumentManifest::new(DocumentKind::Tenants, DocumentRevision::new(1));

    let parsed: Vec<TenantBindingDocument> =
        parse_held_documents(Some(&manifest), Some(b"[]"), DocumentKind::Tenants).unwrap();

    assert!(parsed.is_empty());
}
