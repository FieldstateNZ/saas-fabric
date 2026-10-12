#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::NotADigest;

fn digest() -> RecordDigest {
    RecordDigest::parse(&"ab".repeat(32)).unwrap()
}

#[test]
fn a_report_round_trips_as_exactly_four_keys() {
    let report = ReaderGenerationReport::new(7, digest(), ServedOrigin::Published);
    let json = serde_json::to_value(report).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "contract": 1,
            "generation": 7,
            "record_sha256": "ab".repeat(32),
            "origin": "published",
        })
    );
    let back: ReaderGenerationReport = serde_json::from_value(json).unwrap();
    assert_eq!(back.checked().unwrap(), report);
}

#[test]
fn a_report_with_any_other_key_does_not_decode() {
    let json = serde_json::json!({
        "contract": 1,
        "generation": 7,
        "record_sha256": "ab".repeat(32),
        "origin": "published",
        "tenants": ["acme"],
    });
    assert!(serde_json::from_value::<ReaderGenerationReport>(json).is_err());
}

#[test]
fn the_last_known_good_origin_carries_no_reason() {
    let report = ReaderGenerationReport::new(3, digest(), ServedOrigin::LastKnownGood);
    let json = serde_json::to_value(report).unwrap();
    assert_eq!(json["origin"], "last_known_good");
    let reasoned = serde_json::json!({
        "contract": 1,
        "generation": 3,
        "record_sha256": "ab".repeat(32),
        "origin": { "last_known_good": { "held_because": "torn" } },
    });
    assert!(serde_json::from_value::<ReaderGenerationReport>(reasoned).is_err());
}

#[test]
fn another_contract_or_generation_zero_is_refused() {
    let mut report = ReaderGenerationReport::new(1, digest(), ServedOrigin::Published);
    report.contract = 2;
    assert_eq!(report.checked(), Err(ReportRefusal::UnsupportedContract(2)));
    let zero = ReaderGenerationReport::new(0, digest(), ServedOrigin::Published);
    assert_eq!(zero.checked(), Err(ReportRefusal::NoGeneration));
}

#[test]
fn a_digest_is_sixty_four_lowercase_hex_characters() {
    assert!(RecordDigest::parse(&"0f".repeat(32)).is_ok());
    for refused in [
        "ab".repeat(31),
        "ab".repeat(33),
        "AB".repeat(32),
        format!("{}g", "a".repeat(63)),
        String::new(),
    ] {
        assert_eq!(RecordDigest::parse(&refused), Err(NotADigest), "{refused}");
    }
    let json = serde_json::json!({
        "contract": 1,
        "generation": 1,
        "record_sha256": "not-a-digest",
        "origin": "published",
    });
    assert!(serde_json::from_value::<ReaderGenerationReport>(json).is_err());
}

#[test]
fn the_size_bound_holds_a_report_at_the_largest_generation() {
    let report = ReaderGenerationReport::new(u64::MAX, digest(), ServedOrigin::LastKnownGood);
    assert!(serde_json::to_vec(&report).unwrap().len() < READER_GENERATION_MAX_BYTES);
}

#[test]
fn a_digest_prints_as_it_was_parsed() {
    let text = "0123456789abcdef".repeat(4);
    assert_eq!(RecordDigest::parse(&text).unwrap().to_string(), text);
    assert_eq!(RecordDigest::from_bytes([0xab; 32]), digest());
}
