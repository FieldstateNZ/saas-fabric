//! The provenance verdict over each image's revisions.

use std::collections::BTreeSet;

use fabric_platform_management::Provenance;

use super::revisions::verdict;

fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn one_value_everywhere_is_agreed() {
    assert_eq!(
        verdict(&[set(&["abc"]), set(&["abc"])]),
        Provenance::Agreed("abc".to_owned())
    );
}

#[test]
fn no_images_or_an_image_without_a_value_is_absent() {
    assert_eq!(verdict(&[]), Provenance::Absent);
    assert_eq!(verdict(&[set(&[])]), Provenance::Absent);
    assert_eq!(verdict(&[set(&["abc"]), set(&[])]), Provenance::Absent);
}

#[test]
fn two_values_in_one_image_or_across_images_disagree() {
    assert_eq!(verdict(&[set(&["abc", "def"])]), Provenance::Disagreed);
    assert_eq!(verdict(&[set(&["abc"]), set(&["def"])]), Provenance::Disagreed);
}

#[test]
fn disagreement_wins_over_absence_whatever_the_order() {
    assert_eq!(
        verdict(&[set(&[]), set(&["abc"]), set(&["def"])]),
        Provenance::Disagreed
    );
    assert_eq!(
        verdict(&[set(&["abc"]), set(&["def"]), set(&[])]),
        Provenance::Disagreed
    );
}
