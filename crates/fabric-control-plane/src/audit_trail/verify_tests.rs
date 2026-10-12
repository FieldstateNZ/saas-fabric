//! Verification: an intact chain passes, and each way of changing one is
//! caught as itself.

use fabric_client_model::ClientId;

use super::*;

fn stream() -> AuditStream {
    AuditStream::Client(ClientId::try_new("acme").unwrap())
}

/// A sealed chain of `n` entries.
fn chain(n: u64) -> Vec<AuditEntry> {
    let mut entries: Vec<AuditEntry> = Vec::new();
    for at in 1..=n {
        let record = AuditRecord::new(stream(), "control_plane.audit.test", "test", "operator").with("n", at);
        let head = entries.last().map(AuditEntry::head);
        entries.push(AuditEntry::seal(head.as_ref(), at, record).unwrap());
    }
    entries
}

/// `entry` with one field of its stored JSON replaced, decoded back.
fn edited(entry: &AuditEntry, edit: impl FnOnce(&mut serde_json::Value)) -> AuditEntry {
    let mut value: serde_json::Value = serde_json::from_str(&entry.encode().unwrap()).unwrap();
    edit(&mut value);
    AuditEntry::decode(&value.to_string()).unwrap()
}

#[test]
fn an_intact_chain_verifies_to_its_last_entry() {
    let entries = chain(5);
    let anchor = entries[4].head();

    assert_eq!(verify(&stream(), &entries, Some(&anchor)).unwrap(), Some(anchor));
}

#[test]
fn an_empty_stream_verifies_to_no_head() {
    assert_eq!(verify(&stream(), &[], None).unwrap(), None);
}

#[test]
fn an_edited_entry_is_detected_as_tampered() {
    let mut entries = chain(5);
    entries[2] = edited(&entries[2], |value| {
        value["record"]["requested_by"] = "somebody-else".into();
    });

    assert_eq!(
        verify(&stream(), &entries, None),
        Err(ChainBreak::Tampered { sequence: 3 })
    );
}

#[test]
fn a_dropped_entry_is_detected_as_missing() {
    let mut entries = chain(5);
    entries.remove(2);

    assert_eq!(
        verify(&stream(), &entries, None),
        Err(ChainBreak::Missing {
            expected: 3,
            found: 4
        })
    );
}

#[test]
fn reordered_entries_are_detected_as_out_of_order() {
    let mut entries = chain(5);
    entries.swap(1, 3);

    assert_eq!(
        verify(&stream(), &entries, None),
        Err(ChainBreak::OutOfOrder {
            expected: 2,
            found: 4
        })
    );
}

#[test]
fn reordered_entries_renumbered_to_hide_it_break_the_link() {
    let mut entries = chain(3);
    entries.swap(1, 2);
    entries[1] = edited(&entries[1], |value| value["sequence"] = 2.into());

    assert_eq!(
        verify(&stream(), &entries, None),
        Err(ChainBreak::BrokenLink { sequence: 2 })
    );
}

#[test]
fn a_cut_off_tail_is_detected_against_the_recorded_head() {
    let mut entries = chain(5);
    let anchor = entries[4].head();
    entries.truncate(3);

    assert_eq!(
        verify(&stream(), &entries, Some(&anchor)),
        Err(ChainBreak::Truncated { anchor, found: 3 })
    );
}

#[test]
fn a_chain_rewritten_and_rehashed_is_caught_by_its_head() {
    let entries = chain(3);
    let anchor = entries[2].head();
    let mut rewritten = chain(2);
    let record = AuditRecord::new(stream(), "control_plane.audit.test", "test", "intruder");
    let head = rewritten[1].head();
    rewritten.push(AuditEntry::seal(Some(&head), 3, record).unwrap());

    assert_eq!(
        verify(&stream(), &rewritten, Some(&anchor)),
        Err(ChainBreak::AnchorMismatch { anchor })
    );
}

#[test]
fn one_entry_past_the_recorded_head_is_a_complete_record() {
    let entries = chain(4);

    assert_eq!(
        verify(&stream(), &entries, Some(&entries[2].head())).unwrap(),
        Some(entries[3].head())
    );
}

#[test]
fn an_entry_from_another_stream_is_refused() {
    let mut entries = chain(2);
    let record = AuditRecord::new(
        AuditStream::Catalogue,
        "control_plane.audit.test",
        "test",
        "operator",
    );
    entries.push(AuditEntry::seal(Some(&entries[1].head()), 3, record).unwrap());

    assert_eq!(
        verify(&stream(), &entries, None),
        Err(ChainBreak::ForeignEntry { sequence: 3 })
    );
}
