use super::*;

const EVERY_PLACEMENT: [PlacementClassDocument; 6] = [
    PlacementClassDocument::Shared,
    PlacementClassDocument::Dedicated,
    PlacementClassDocument::HighAvailability,
    PlacementClassDocument::Regulated,
    PlacementClassDocument::Development,
    PlacementClassDocument::Ephemeral,
];

#[test]
fn every_placement_round_trips_through_its_console_word() {
    for placement in EVERY_PLACEMENT {
        let word = console_word(placement);
        let parsed = parse_word(word).unwrap();

        assert_eq!(parsed, placement, "{word}");
    }
}

#[test]
fn high_availability_is_camel_case_not_the_wires_snake_case() {
    assert_eq!(
        console_word(PlacementClassDocument::HighAvailability),
        "highAvailability"
    );
    assert!(
        parse_word("high_availability").is_err(),
        "the wire's own spelling is not a console word"
    );
}

#[test]
fn an_unknown_word_is_refused_as_a_malformed_request() {
    let error = parse_word("nonexistent").unwrap_err();

    assert!(matches!(error, ControlPlaneError::InvalidRequest(_)));
}
