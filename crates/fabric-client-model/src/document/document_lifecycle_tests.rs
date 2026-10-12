//! `spec.lifecycle` as stored: read, written, and refused when a hand edit
//! breaks its invariants.

use crate::{ClientDocument, ClientId, DesiredStateError, LifecycleRecord};

const ACME: &str = r"
apiVersion: fabric.fieldstate.nz/v2
kind: Client
metadata:
  name: acme
spec:
  displayName: Acme
  identity:
    realm: acme
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients: []
  features:
    invoicing: true
";

fn acme() -> ClientDocument {
    ClientDocument::parse(ACME).unwrap()
}

fn with_lifecycle(section: &str) -> Result<ClientDocument, DesiredStateError> {
    ClientDocument::parse(&format!("{ACME}  lifecycle:\n{section}"))
}

fn id(value: &str) -> ClientId {
    ClientId::try_new(value).unwrap()
}

#[test]
fn a_document_without_a_lifecycle_section_is_saved_intent() {
    assert_eq!(acme().lifecycle(), &LifecycleRecord::default());
    assert!(!acme().lifecycle().is_retired());
}

#[test]
fn a_tombstone_is_written_and_read_back_and_other_sections_survive() {
    let record = LifecycleRecord::default()
        .tombstone(100, "operator-1", "Contract ended")
        .unwrap();

    let written = acme().with_lifecycle(&record).unwrap();
    let reread = ClientDocument::parse(&written.render().unwrap()).unwrap();

    assert_eq!(reread.lifecycle(), &record);
    assert!(reread.render().unwrap().contains("invoicing: true"));
}

#[test]
fn a_confirmed_deletion_is_written_and_read_back() {
    let record = LifecycleRecord::default()
        .tombstone(100, "operator-1", "Contract ended")
        .unwrap()
        .confirm_deletion(&id("acme"), &id("acme"), 200, "operator-2")
        .unwrap();

    let written = acme().with_lifecycle(&record).unwrap();

    assert_eq!(written.lifecycle(), &record);
}

#[test]
fn a_hand_edit_confirming_deletion_without_a_tombstone_is_refused() {
    let error = with_lifecycle("    deletionConfirmed:\n      at: 200\n      by: operator-2\n").unwrap_err();

    assert!(matches!(
        error,
        DesiredStateError::InvalidField {
            field: "spec.lifecycle",
            ..
        }
    ));
}

#[test]
fn a_hand_edit_confirming_deletion_before_the_tombstone_is_refused() {
    let error = with_lifecycle(
        "    tombstone:\n      at: 300\n      by: o\n      reason: r\n    deletionConfirmed:\n      at: 200\n      by: o\n",
    )
    .unwrap_err();

    assert!(matches!(error, DesiredStateError::InvalidField { .. }));
}

#[test]
fn a_hand_edit_with_a_blank_tombstone_reason_is_refused() {
    let error =
        with_lifecycle("    tombstone:\n      at: 300\n      by: o\n      reason: ' '\n").unwrap_err();

    assert!(matches!(error, DesiredStateError::InvalidField { .. }));
}

#[test]
fn a_misspelt_lifecycle_key_is_refused_rather_than_read_as_active() {
    let error = with_lifecycle("    tombstoned:\n      at: 300\n      by: o\n      reason: r\n").unwrap_err();

    assert!(matches!(error, DesiredStateError::Malformed { .. }));
}

#[test]
fn a_hand_written_tombstone_parses_as_retired() {
    let document = with_lifecycle("    tombstone:\n      at: 300\n      by: o\n      reason: r\n").unwrap();

    assert!(document.lifecycle().is_retired());
}

// Restorations as stored.

const RESTORED: &str = "    restorations:\n      - tombstone:\n          at: 100\n          by: o\n          reason: r\n        at: 150\n        by: p\n        reason: renewed\n";

#[test]
fn a_restoration_is_written_and_read_back_and_other_sections_survive() {
    let record = LifecycleRecord::default()
        .tombstone(100, "operator-1", "Contract ended")
        .unwrap()
        .restore(150, "operator-3", "Contract renewed")
        .unwrap();

    let written = acme().with_lifecycle(&record).unwrap();
    let reread = ClientDocument::parse(&written.render().unwrap()).unwrap();

    assert_eq!(reread.lifecycle(), &record);
    assert!(!reread.lifecycle().is_retired());
    assert!(reread.render().unwrap().contains("invoicing: true"));
    assert_ne!(
        reread.render().unwrap(),
        acme().render().unwrap(),
        "a restored document never reads as the one before its tombstone"
    );
}

#[test]
fn a_hand_written_restoration_in_order_parses() {
    let document = with_lifecycle(RESTORED).unwrap();

    assert_eq!(document.lifecycle().restorations.len(), 1);
    assert!(!document.lifecycle().is_retired());
}

#[test]
fn a_restoration_before_the_tombstone_it_reverses_is_refused() {
    let error = with_lifecycle(&RESTORED.replace("at: 150", "at: 50")).unwrap_err();

    assert!(
        matches!(error, DesiredStateError::InvalidField { .. }),
        "{error:?}"
    );
}

#[test]
fn restorations_out_of_order_are_refused() {
    let second = "      - tombstone:\n          at: 120\n          by: o\n          reason: r\n        at: 160\n        by: p\n        reason: renewed\n";
    let error = with_lifecycle(&format!("{RESTORED}{second}")).unwrap_err();

    assert!(
        matches!(error, DesiredStateError::InvalidField { .. }),
        "{error:?}"
    );
}

#[test]
fn a_tombstone_before_the_last_restoration_is_refused() {
    let tombstone = "    tombstone:\n      at: 140\n      by: o\n      reason: r\n";
    let error = with_lifecycle(&format!("{tombstone}{RESTORED}")).unwrap_err();

    assert!(
        matches!(error, DesiredStateError::InvalidField { .. }),
        "{error:?}"
    );
}

#[test]
fn a_restoration_with_a_blank_reason_or_unknown_key_is_refused() {
    let blank = with_lifecycle(&RESTORED.replace("reason: renewed", "reason: ' '")).unwrap_err();
    assert!(
        matches!(blank, DesiredStateError::InvalidField { .. }),
        "{blank:?}"
    );

    let misspelt = with_lifecycle(&RESTORED.replace("by: p", "restoredBy: p")).unwrap_err();
    assert!(
        matches!(misspelt, DesiredStateError::Malformed { .. }),
        "{misspelt:?}"
    );
}
