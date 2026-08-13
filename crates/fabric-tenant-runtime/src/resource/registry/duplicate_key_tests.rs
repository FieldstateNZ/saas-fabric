//! Two entries for the same key inside one incoming set.
//!
//! A source is supposed to publish a *set*. Nothing stops it publishing a list
//! with the same key twice — `JsonFileSource` deserialises a JSON array
//! straight into a `Vec<T>` — and when it does, every per-resource decision in
//! one `apply_all` must still add up to a single coherent story.
//!
//! The invariant these pin down is deliberately stronger than the two reported
//! reproductions: **every published event describes a transition into the
//! snapshot that was actually installed.** A subscriber's whole job (§19) is to
//! drop state attached to the old revision and re-read the new one, so an event
//! naming a snapshot that never existed sends it to look up something that is
//! not there.

use std::collections::HashMap;

use fabric_core::BindingRevision;

use crate::resource::registry::test_resource::{
    invalid_resource, registry, resource, resource_with_payload, TestResource,
};
use crate::resource::{ResourceChange, ResourceRegistry};

/// Applies `incoming`, then checks every event it published against the
/// snapshot that ended up installed.
///
/// `keys` names every key the case touches. Both endpoints of an event are
/// checked, not just the destination: an `Updated` event is only honest if the
/// revision it claims to have moved *from* is the one that was really held
/// before the apply.
fn assert_events_describe_the_installed_snapshot(
    registry: &ResourceRegistry<TestResource>,
    keys: &[&str],
    incoming: Vec<TestResource>,
) {
    let before: HashMap<String, Option<BindingRevision>> = keys
        .iter()
        .map(|key| {
            let held = registry.lookup(&(*key).to_owned()).ok();
            ((*key).to_owned(), held.map(|held| held.revision))
        })
        .collect();

    let mut changes = registry.subscribe();
    registry.apply_all(incoming);

    let mut events: Vec<ResourceChange<String>> = Vec::new();
    while let Ok(event) = changes.try_recv() {
        events.push(event);
    }

    let mut announced: Vec<String> = Vec::new();
    for event in events {
        assert!(
            !announced.contains(&event.key),
            "two events were published for {} in one apply; only one of them can \
             possibly describe the single snapshot that was installed",
            event.key
        );

        let installed = registry.lookup(&event.key).ok().map(|held| held.revision);
        assert_eq!(
            event.current_revision, installed,
            "the {:?} event for {} says the registry now holds {:?}, but it holds {:?}",
            event.kind, event.key, event.current_revision, installed
        );

        assert_eq!(
            Some(&event.previous_revision),
            before.get(&event.key),
            "the {:?} event for {} says it moved from {:?}, which is not what was held \
             before the apply",
            event.kind,
            event.key,
            event.previous_revision
        );

        announced.push(event.key);
    }
}

#[test]
fn a_duplicate_never_publishes_an_event_for_a_snapshot_that_never_existed() {
    // Reproduction one. The invalid second entry is compared against the *old*
    // snapshot rather than against the entry that just won, so it restores the
    // held copy over the top of it — while the first entry's event stays
    // queued and is published anyway.
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]);

    assert_events_describe_the_installed_snapshot(
        &registry,
        &["a"],
        vec![resource("a", 5), invalid_resource("a", 6)],
    );
}

#[test]
fn two_entries_for_one_key_never_publish_contradictory_transitions() {
    // Reproduction two. Both entries beat the held revision 1, so both are
    // announced as updates — 1→9 and 1→3 — and the later one wins the map.
    // "Revisions only move forward" (§20) has to hold *within* one apply too.
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]);

    assert_events_describe_the_installed_snapshot(
        &registry,
        &["a"],
        vec![resource("a", 9), resource("a", 3)],
    );
}

#[test]
fn the_invariant_holds_for_every_shape_a_repeated_key_can_take() {
    // The invariant is a property of `apply_all`, not a fact about two
    // reported inputs, so it is checked against every combination a second
    // entry for one key can be: newer, older, equal, divergent, invalid, in
    // both orders, and alongside a key that appears only once.
    let cases: Vec<Vec<TestResource>> = vec![
        vec![resource("a", 5), resource("a", 6)],
        vec![resource("a", 6), resource("a", 5)],
        vec![resource("a", 5), resource("a", 3)],
        vec![resource("a", 3), resource("a", 5)],
        vec![resource("a", 5), invalid_resource("a", 6)],
        vec![invalid_resource("a", 5), resource("a", 6)],
        vec![resource("a", 4), resource_with_payload("a", 4, "divergent")],
        vec![resource("a", 5), resource("a", 5)],
        vec![resource("a", 5), resource("a", 6), resource("b", 1)],
        vec![resource("b", 1), resource("b", 2)],
    ];

    for incoming in cases {
        let registry = registry();
        registry.apply_all(vec![resource("a", 4)]);

        assert_events_describe_the_installed_snapshot(&registry, &["a", "b"], incoming);
    }
}

#[test]
fn a_refused_duplicate_is_counted_rather_than_folded_into_another_bucket() {
    // The whole point of a distinct counter: a source disagreeing with itself
    // must not hide inside `unchanged`, `stale_ignored`, or `invalid_rejected`,
    // all of which describe the source disagreeing with what is *held*.
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]);

    let report = registry.apply_all(vec![resource("a", 2), resource("a", 3), resource("b", 1)]);

    assert_eq!(report.duplicate_rejected, 1);
    assert_eq!(report.updated, 1);
    assert_eq!(report.added, 1);
    assert_eq!(report.stale_ignored, 0);
    assert_eq!(report.unchanged, 0);
    assert_eq!(report.invalid_rejected, 0);
    assert_eq!(report.divergent_payload, 0);
}

#[test]
fn a_repeated_key_does_not_stop_its_neighbours_loading() {
    // Same rule as an invalid resource: one reconciler bug must not freeze
    // every other resource's updates.
    let registry = registry();

    registry.apply_all(vec![resource("a", 1), resource("a", 2), resource("b", 7)]);

    assert_eq!(registry.len(), 2);
    assert_eq!(
        registry.lookup(&"b".to_owned()).unwrap().revision,
        BindingRevision::new(7)
    );
}

#[test]
fn the_first_entry_for_a_key_decides_its_fate() {
    // Position, not revision, picks the winner. Choosing by revision would
    // mean interpreting data the source has already got wrong, and would make
    // the outcome depend on the very field the duplicate calls into question.
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]);

    registry.apply_all(vec![resource("a", 9), resource("a", 3)]);

    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap().revision,
        BindingRevision::new(9)
    );
}
