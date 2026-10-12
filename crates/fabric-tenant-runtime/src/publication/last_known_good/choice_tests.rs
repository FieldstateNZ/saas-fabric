//! Every hold and fail-closed rule, one case each.

use super::choice::{at_start, on_refresh, Choice};
use super::fixtures::complete;
use crate::SourceError;

fn torn() -> SourceError {
    SourceError::IncompleteGeneration {
        origin: "published".to_owned(),
        generation: 9,
        document: "data-sources",
    }
}

fn applied(choice: &Choice) -> String {
    match choice {
        Choice::Published(generation) => format!("published {}", generation.generation),
        Choice::LastKnownGood { generation, .. } => format!("last-known-good {}", generation.generation),
        Choice::Unrecorded => "unrecorded".to_owned(),
        Choice::Refused(error) => format!("refused: {error}"),
    }
}

#[test]
fn at_start_a_torn_volume_is_held_against_with_a_valid_copy() {
    let choice = at_start(Err(torn()), Some(complete(4, "pg-1")));
    assert_eq!(applied(&choice), "last-known-good 4");
    let Choice::LastKnownGood { held_because, .. } = choice else {
        unreachable!()
    };
    assert!(held_because.contains("data-sources"), "{held_because}");
}

#[test]
fn at_start_a_torn_volume_with_no_copy_fails_closed() {
    let choice = at_start(Err(torn()), None);
    assert!(matches!(
        choice,
        Choice::Refused(SourceError::IncompleteGeneration { .. })
    ));
}

#[test]
fn at_start_a_newer_published_generation_beats_the_copy() {
    assert_eq!(
        applied(&at_start(
            Ok(Some(complete(5, "pg-2"))),
            Some(complete(4, "pg-1"))
        )),
        "published 5"
    );
}

#[test]
fn at_start_the_same_generation_published_is_taken_from_the_published_files() {
    assert_eq!(
        applied(&at_start(
            Ok(Some(complete(4, "pg-1"))),
            Some(complete(4, "pg-1"))
        )),
        "published 4"
    );
}

#[test]
fn at_start_a_published_generation_below_the_copy_does_not_roll_the_replica_back() {
    assert_eq!(
        applied(&at_start(
            Ok(Some(complete(3, "pg-1"))),
            Some(complete(4, "pg-1"))
        )),
        "last-known-good 4"
    );
}

#[test]
fn at_start_a_lost_record_is_held_against_with_a_copy_and_read_per_document_without_one() {
    assert_eq!(
        applied(&at_start(Ok(None), Some(complete(4, "pg-1")))),
        "last-known-good 4"
    );
    assert_eq!(applied(&at_start(Ok(None), None)), "unrecorded");
}

#[test]
fn at_start_with_no_copy_a_published_generation_is_applied() {
    assert_eq!(
        applied(&at_start(Ok(Some(complete(1, "pg-1"))), None)),
        "published 1"
    );
}

#[test]
fn on_refresh_a_newer_or_equal_generation_is_applied() {
    assert_eq!(
        applied(&on_refresh(Ok(Some(complete(5, "pg-1"))), Some(4), "p")),
        "published 5"
    );
    assert_eq!(
        applied(&on_refresh(Ok(Some(complete(4, "pg-1"))), Some(4), "p")),
        "published 4"
    );
}

#[test]
fn on_refresh_a_lower_generation_or_a_lost_record_is_held_against() {
    for (published, found) in [
        (Some(complete(3, "pg-1")), "generation 3"),
        (None, "no publication record"),
    ] {
        let choice = on_refresh(Ok(published), Some(4), "p");
        let Choice::Refused(SourceError::Regressed {
            held, found: said, ..
        }) = choice
        else {
            panic!("expected a regression refusal, got {}", applied(&choice));
        };
        assert_eq!((held, said.as_str()), (4, found));
    }
}

#[test]
fn on_refresh_before_anything_recorded_was_held_the_reader_behaves_as_without_a_copy() {
    assert_eq!(applied(&on_refresh(Ok(None), None, "p")), "unrecorded");
    assert!(matches!(on_refresh(Err(torn()), None, "p"), Choice::Refused(_)));
}
