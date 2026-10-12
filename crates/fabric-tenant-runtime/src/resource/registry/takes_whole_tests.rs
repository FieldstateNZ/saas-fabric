//! `refuses_whole` agrees with what `apply_all` would take, case by case,
//! and writes nothing.

use super::super::test_resource::{
    invalid_resource, registry, resource, resource_with_payload, TestResource,
};
use crate::resource::ResourceRegistry;

fn held() -> ResourceRegistry<TestResource> {
    let registry = registry();
    registry
        .apply_all(vec![resource("a", 2), resource("b", 1)])
        .unwrap();
    registry
}

/// Asks first, then applies, and checks both answers agree.
fn judged(registry: &ResourceRegistry<TestResource>, incoming: &[TestResource]) -> Option<String> {
    let refusal = registry.refuses_whole(incoming);
    let report = registry.apply_all(incoming.to_vec()).unwrap();
    assert_eq!(
        refusal.is_none(),
        report.took_every_resource(),
        "{refusal:?} {report:?}"
    );
    refusal
}

#[test]
fn a_set_every_resource_of_which_would_be_taken_is_not_refused() {
    let incoming = [resource("a", 3), resource("b", 1), resource("c", 1)];

    assert_eq!(judged(&held(), &incoming), None);
}

#[test]
fn an_invalid_resource_is_refused() {
    let refusal = judged(&held(), &[resource("a", 3), invalid_resource("b", 2)]).unwrap();

    assert!(refusal.contains("deliberately invalid"), "{refusal}");
}

#[test]
fn a_key_twice_is_refused() {
    let refusal = judged(&held(), &[resource("c", 1), resource("c", 2)]).unwrap();

    assert!(refusal.contains("appears twice"), "{refusal}");
}

#[test]
fn a_revision_below_the_one_applied_is_refused() {
    let refusal = judged(&held(), &[resource("a", 1), resource("b", 1)]).unwrap();

    assert!(refusal.contains("below revision 2"), "{refusal}");
}

#[test]
fn a_changed_payload_at_the_revision_applied_is_refused() {
    let refusal = judged(
        &held(),
        &[resource("a", 2), resource_with_payload("b", 1, "changed")],
    )
    .unwrap();

    assert!(refusal.contains("without its revision 1 advancing"), "{refusal}");
}

#[test]
fn asking_writes_nothing() {
    let registry = held();

    assert!(registry.refuses_whole(&[resource("a", 1)]).is_some());
    assert_eq!(registry.len(), 2);
    assert_eq!(registry.lookup(&"a".to_owned()).unwrap().revision.get(), 2);
}
