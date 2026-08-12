//! Applying reconciled state, and the revision guard that protects it.

use fabric_core::BindingRevision;

use crate::resource::registry::test_resource::{registry, resource, resource_with_payload};

#[test]
fn a_full_sync_removes_resources_absent_from_the_incoming_set() {
    let registry = registry();
    registry.apply_all(vec![resource("a", 1), resource("b", 1)]);

    let report = registry.apply_all(vec![resource("a", 1)]);

    assert_eq!(report.removed, 1);
    assert!(registry.lookup(&"b".to_owned()).is_err());
}
#[test]
fn an_older_revision_is_ignored_rather_than_resurrecting_retired_state() {
    // A stale read must not point a tenant back at a database a migration has
    // already drained.
    let registry = registry();
    registry.apply_all(vec![resource("a", 10)]);

    let report = registry.apply_all(vec![resource("a", 3)]);

    assert_eq!(report.stale_ignored, 1);
    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap().revision,
        BindingRevision::new(10)
    );
}
#[test]
fn an_identical_revision_is_reported_as_unchanged() {
    // Item 50, the quiet case: same revision, same payload.
    let registry = registry();
    registry.apply_all(vec![resource("a", 5)]);

    let report = registry.apply_all(vec![resource("a", 5)]);

    assert_eq!(report.unchanged, 1);
    assert_eq!(report.divergent_payload, 0);
    assert!(report.is_noop());
}
#[test]
fn a_same_revision_divergent_payload_is_counted_and_the_held_payload_is_kept() {
    // Item 50, the loud case: same revision, different payload. A reconciler
    // that changed content but forgot to bump the revision must not vanish
    // without trace — it is counted here, and warn-logged in `apply_all`.
    let registry = registry();
    registry.apply_all(vec![resource_with_payload("a", 5, "original")]);

    let report = registry.apply_all(vec![resource_with_payload("a", 5, "changed")]);

    assert_eq!(report.divergent_payload, 1);
    assert_eq!(report.unchanged, 0);
    assert_eq!(registry.lookup(&"a".to_owned()).unwrap().payload, "original");
}
#[test]
fn applying_one_resource_leaves_the_others_alone() {
    let registry = registry();
    registry.apply_all(vec![resource("a", 1), resource("b", 1)]);

    assert!(registry.apply_one(resource("a", 2)));

    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap().revision,
        BindingRevision::new(2)
    );
    assert_eq!(
        registry.lookup(&"b".to_owned()).unwrap().revision,
        BindingRevision::new(1)
    );
}
#[test]
fn applying_one_resource_at_the_same_revision_is_refused() {
    let registry = registry();
    registry.apply_all(vec![resource("a", 4)]);

    assert!(!registry.apply_one(resource("a", 4)));
}
#[test]
fn applying_one_resource_with_a_divergent_payload_at_the_same_revision_is_refused() {
    // The `apply_one` counterpart to item 50: the same-revision guard
    // refuses the update even though the payload disagrees, and the held
    // payload is left exactly as it was.
    let registry = registry();
    registry.apply_all(vec![resource_with_payload("a", 4, "original")]);

    assert!(!registry.apply_one(resource_with_payload("a", 4, "changed")));
    assert_eq!(registry.lookup(&"a".to_owned()).unwrap().payload, "original");
}
#[test]
fn invalidating_a_resource_makes_it_fail_closed() {
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]);

    assert!(registry.invalidate(&"a".to_owned()));
    assert!(registry.lookup(&"a".to_owned()).is_err());
    // One resource went away; the registry is still primed and serving.
    assert!(registry.is_primed());
}
#[test]
fn invalidating_an_absent_resource_reports_that_nothing_happened() {
    let registry = registry();
    registry.apply_all(vec![]);

    assert!(!registry.invalidate(&"a".to_owned()));
}
