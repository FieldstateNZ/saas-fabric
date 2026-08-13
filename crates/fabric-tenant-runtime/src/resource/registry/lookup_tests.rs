//! Looking a resource up, and the unprimed-versus-absent distinction.

use fabric_core::BindingRevision;

use crate::resource::registry::test_resource::{registry, resource};
use crate::resource::LookupError;

#[test]
fn an_unprimed_registry_reports_unavailable_not_not_found() {
    // The distinction is the whole point: a cold start must not look like a
    // deleted resource.
    let registry = registry();

    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap_err(),
        LookupError::Unavailable
    );
    assert!(!registry.is_primed());
}
#[test]
fn a_primed_registry_reports_an_absent_key_as_not_found() {
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]).unwrap();

    assert_eq!(
        registry.lookup(&"b".to_owned()).unwrap_err(),
        LookupError::NotFound
    );
}
#[test]
fn priming_with_an_empty_set_still_counts_as_primed() {
    let registry = registry();
    registry.apply_all(vec![]).unwrap();

    assert!(registry.is_primed());
    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap_err(),
        LookupError::NotFound
    );
}
#[test]
fn resolves_a_held_resource() {
    let registry = registry();
    registry.apply_all(vec![resource("a", 7)]).unwrap();

    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap().revision,
        BindingRevision::new(7)
    );
}
