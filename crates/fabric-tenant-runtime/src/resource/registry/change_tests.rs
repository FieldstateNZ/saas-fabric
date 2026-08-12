//! Change notification, and its ordering guarantee.

use fabric_core::BindingRevision;

use crate::resource::registry::test_resource::{registry, resource};
use crate::resource::ChangeKind;

#[tokio::test]
async fn subscribers_are_told_when_a_resource_advances() {
    let registry = registry();
    let mut changes = registry.subscribe();

    registry.apply_all(vec![resource("a", 1)]);
    registry.apply_one(resource("a", 2));

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
    registry.apply_all(vec![resource("a", 3)]);

    let mut changes = registry.subscribe();
    registry.invalidate(&"a".to_owned());

    let removed = changes.recv().await.unwrap();
    assert_eq!(removed.kind, ChangeKind::Removed);
    assert_eq!(removed.current_revision, None);
}
#[tokio::test]
async fn a_change_is_published_only_after_the_new_state_is_visible() {
    // A subscriber that reacts by looking the resource up must not see the old
    // value.
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]);

    let mut changes = registry.subscribe();
    registry.apply_one(resource("a", 2));

    let change = changes.recv().await.unwrap();
    assert_eq!(change.current_revision, Some(BindingRevision::new(2)));
    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap().revision,
        BindingRevision::new(2)
    );
}
