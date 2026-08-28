//! Change notification, and its ordering guarantee.
//!
//! Every mutation below is asserted rather than discarded. `apply_one` and
//! `invalidate` publish an event only on their `true` path, so a silently
//! refused mutation leaves `changes.recv().await` waiting forever — these
//! tests would hang rather than fail, which is the worse of the two.

use fabric_core::BindingRevision;

use crate::resource::registry::test_resource::{registry, resource};
use crate::resource::ChangeKind;

#[tokio::test]
async fn subscribers_are_told_when_a_resource_advances() {
    let registry = registry();
    let mut changes = registry.subscribe();

    registry.apply_all(vec![resource("a", 1)]).unwrap();
    assert!(registry.apply_one(resource("a", 2)));

    let added = changes.recv().await.unwrap();
    assert_eq!(added.kind, ChangeKind::Added);
    assert_eq!(added.current_revision, Some(BindingRevision::new(1)));

    let updated = changes.recv().await.unwrap();
    assert_eq!(updated.kind, ChangeKind::Updated);
    assert_eq!(updated.previous_revision, Some(BindingRevision::new(1)));
}
#[tokio::test]
async fn subscribers_are_told_when_a_resource_is_removed() {
    let registry = registry();
    registry.apply_all(vec![resource("a", 3)]).unwrap();

    let mut changes = registry.subscribe();
    assert!(registry.invalidate(&"a".to_owned()));

    let removed = changes.recv().await.unwrap();
    assert_eq!(removed.kind, ChangeKind::Removed);
    assert_eq!(removed.current_revision, None);
}
#[tokio::test]
async fn a_change_is_published_only_after_the_new_state_is_visible() {
    // A subscriber that reacts by looking the resource up must not see the old
    // value.
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]).unwrap();

    let mut changes = registry.subscribe();
    assert!(registry.apply_one(resource("a", 2)));

    let change = changes.recv().await.unwrap();
    assert_eq!(change.current_revision, Some(BindingRevision::new(2)));
    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap().revision,
        BindingRevision::new(2)
    );
}
