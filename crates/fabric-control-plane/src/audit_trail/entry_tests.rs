//! Sealing, the size cap, and concurrent appends through the port.

use std::sync::Arc;

use super::*;

fn record(note: &str) -> AuditRecord {
    AuditRecord::new(
        AuditStream::Catalogue,
        "control_plane.audit.test",
        "test",
        "operator",
    )
    .with("note", note)
}

#[test]
fn the_first_entry_starts_at_one_after_genesis() {
    let entry = AuditEntry::seal(None, 10, record("a")).unwrap();

    assert_eq!(entry.sequence(), 1);
    assert_eq!(entry.previous(), ChainHash::GENESIS);
    assert_eq!(entry.expected_hash().unwrap(), entry.hash());
}

#[test]
fn an_entry_round_trips_through_its_line() {
    let entry = AuditEntry::seal(None, 10, record("a")).unwrap();

    assert_eq!(AuditEntry::decode(&entry.encode().unwrap()).unwrap(), entry);
}

#[test]
fn an_oversize_entry_is_refused_whole_rather_than_shortened() {
    let error = AuditEntry::seal(None, 10, record(&"x".repeat(MAX_AUDIT_ENTRY_BYTES))).unwrap_err();

    assert!(matches!(
        error,
        AuditError::EntryTooLarge { size, limit } if size > limit && limit == MAX_AUDIT_ENTRY_BYTES
    ));
}

#[test]
fn a_line_carrying_an_extra_field_is_not_an_entry() {
    let line = AuditEntry::seal(None, 10, record("a")).unwrap().encode().unwrap();
    let padded = line.replacen('{', "{\"extra\":1,", 1);

    assert!(matches!(
        AuditEntry::decode(&padded),
        Err(AuditError::Malformed { .. })
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_appends_to_one_stream_keep_one_unbroken_chain() {
    let trail = Arc::new(InMemoryAuditTrail::new());
    let appends = (0..64).map(|n| {
        let trail = Arc::clone(&trail);
        tokio::spawn(async move { trail.append(record(&n.to_string()), n).await })
    });
    for append in appends {
        append.await.unwrap().unwrap();
    }

    let stored = trail.read(&AuditStream::Catalogue).await.unwrap();

    assert_eq!(stored.entries.len(), 64);
    assert_eq!(stored.verify().unwrap().map(|head| head.sequence), Some(64));
}

#[tokio::test]
async fn an_oversize_record_appends_nothing_to_its_stream() {
    let trail = InMemoryAuditTrail::new();
    trail.append(record("kept"), 1).await.unwrap();

    let refused = trail.append(record(&"x".repeat(MAX_AUDIT_ENTRY_BYTES)), 2).await;
    let stored = trail.read(&AuditStream::Catalogue).await.unwrap();

    assert!(matches!(refused, Err(AuditError::EntryTooLarge { .. })));
    assert_eq!(stored.entries.len(), 1);
    stored.verify().unwrap();
}
