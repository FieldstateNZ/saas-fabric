//! Which tags a picker offers, and in which order.

use super::versions::sorted;

#[test]
fn versions_are_ordered_by_precedence_newest_first_and_the_rest_counted() {
    let tags = [
        "0.3.0-preview.9",
        "latest",
        "0.3.0-preview.10",
        "0.2.1",
        "v0.4.0",
        "sha256-0123",
        "0.3.0",
        "1.0.0+build.1",
    ]
    .map(str::to_owned)
    .to_vec();

    let found = sorted(tags);

    assert_eq!(
        found.tags,
        ["0.3.0", "0.3.0-preview.10", "0.3.0-preview.9", "0.2.1"]
    );
    assert_eq!(found.other, 4);
}

#[test]
fn no_tags_is_no_versions_and_nothing_else() {
    let found = sorted(Vec::new());
    assert!(found.tags.is_empty());
    assert_eq!(found.other, 0);
}
