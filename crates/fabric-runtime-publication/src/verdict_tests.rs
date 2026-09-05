//! One test per row of ADR 0018's presence table and revision table.

use crate::verdict::{verdict, Held, Incoming, Verdict};
use crate::{DocumentKind, DocumentRevision, PublicationError};

fn incoming(revision: u64, payload: &[u8]) -> Incoming<'_> {
    Incoming {
        document: DocumentKind::Tenants,
        revision: DocumentRevision::new(revision),
        payload,
    }
}

#[test]
fn no_manifest_and_no_payload_is_a_first_publication_that_writes() {
    assert_eq!(verdict(None, &incoming(1, b"[]")).unwrap(), Verdict::Write);
}

#[test]
fn no_manifest_but_an_orphaned_payload_still_writes_with_the_guard_off() {
    // The shipped `examples/*.json` today: a payload ships with no manifest
    // beside it. Whatever that file already contains is irrelevant here --
    // there is no held revision to compare against.
    assert_eq!(verdict(None, &incoming(1, b"[]")).unwrap(), Verdict::Write);
}

#[test]
fn a_held_manifest_without_its_payload_is_republishable_at_the_same_revision() {
    let held = Held {
        revision: DocumentRevision::new(3),
        payload: None,
    };

    assert_eq!(verdict(Some(held), &incoming(3, b"[]")).unwrap(), Verdict::Write);
}

#[test]
fn a_held_manifest_without_its_payload_still_writes_at_a_newer_revision() {
    let held = Held {
        revision: DocumentRevision::new(3),
        payload: None,
    };

    assert_eq!(verdict(Some(held), &incoming(4, b"[]")).unwrap(), Verdict::Write);
}

#[test]
fn a_held_manifest_without_its_payload_still_refuses_an_older_revision() {
    // The byte comparison has nothing to run against with the payload gone,
    // but the manifest still states a revision -- an offered revision older
    // than *that* is stale regardless of what the payload would have said.
    let held = Held {
        revision: DocumentRevision::new(3),
        payload: None,
    };

    let error = verdict(Some(held), &incoming(2, b"[]")).unwrap_err();

    assert!(
        matches!(error, PublicationError::StaleRevision { held, offered, .. }
        if held == DocumentRevision::new(3) && offered == DocumentRevision::new(2))
    );
}

#[test]
fn an_older_revision_against_a_held_manifest_and_payload_is_refused() {
    let held = Held {
        revision: DocumentRevision::new(5),
        payload: Some(b"[]"),
    };

    let error = verdict(Some(held), &incoming(4, b"[]")).unwrap_err();

    assert!(
        matches!(error, PublicationError::StaleRevision { held, offered, .. }
        if held == DocumentRevision::new(5) && offered == DocumentRevision::new(4))
    );
}

#[test]
fn the_same_revision_with_different_bytes_is_refused_as_divergent() {
    let held = Held {
        revision: DocumentRevision::new(5),
        payload: Some(b"[\"old\"]"),
    };

    let error = verdict(Some(held), &incoming(5, b"[\"new\"]")).unwrap_err();

    assert!(
        matches!(error, PublicationError::DivergentPayload { revision, .. }
        if revision == DocumentRevision::new(5))
    );
}

#[test]
fn the_same_revision_with_identical_bytes_is_a_no_op() {
    let held = Held {
        revision: DocumentRevision::new(5),
        payload: Some(b"[\"same\"]"),
    };

    assert_eq!(
        verdict(Some(held), &incoming(5, b"[\"same\"]")).unwrap(),
        Verdict::Unchanged
    );
}

#[test]
fn a_newer_revision_against_a_held_manifest_and_payload_writes() {
    let held = Held {
        revision: DocumentRevision::new(5),
        payload: Some(b"[\"old\"]"),
    };

    assert_eq!(
        verdict(Some(held), &incoming(6, b"[\"new\"]")).unwrap(),
        Verdict::Write
    );
}
