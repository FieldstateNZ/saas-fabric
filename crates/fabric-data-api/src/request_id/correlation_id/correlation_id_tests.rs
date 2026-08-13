//! Which inbound correlation ids are adopted, and what replaces the rest.

use super::*;

/// A `HeaderMap` carrying one `X-Request-Id`.
fn headers_with(id: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(HEADER, HeaderValue::from_str(id).unwrap());

    headers
}

/// Whether the id chosen for these headers was generated here rather than
/// taken from the caller.
fn was_generated(headers: &HeaderMap) -> bool {
    uuid::Uuid::parse_str(&for_request(headers)).is_ok()
}

#[test]
fn an_absent_header_produces_a_generated_id() {
    assert!(was_generated(&HeaderMap::new()));
}

#[test]
fn an_inbound_header_is_propagated_unchanged() {
    assert_eq!(
        for_request(&headers_with("caller-supplied-id")),
        "caller-supplied-id"
    );
}

#[test]
fn an_empty_inbound_header_is_treated_as_absent() {
    assert!(was_generated(&headers_with("")));
}

#[test]
fn the_id_formats_callers_actually_send_are_all_adopted() {
    // A UUID, a W3C `traceparent`, an X-Ray trace id, and a base64-shaped
    // gateway id. If the allowlist ever tightens, these are what it must not
    // start rejecting.
    for id in [
        "3f8a1c2e-9b4d-4e6f-8a0b-1d2c3e4f5a6b",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        "1-58406520-a006649127e371903a2de979",
        "aGVsbG8gd29ybGQ=/x+y",
    ] {
        assert_eq!(for_request(&headers_with(id)), id, "{id} must be adopted");
    }
}

#[test]
fn an_id_at_the_length_limit_is_adopted() {
    let id = "a".repeat(MAX_LENGTH);

    assert_eq!(for_request(&headers_with(&id)), id);
}

#[test]
fn an_id_one_character_over_the_length_limit_is_replaced() {
    assert!(was_generated(&headers_with(&"a".repeat(MAX_LENGTH + 1))));
}

#[test]
fn a_megabyte_of_id_is_neither_adopted_nor_trimmed() {
    // The defect this bound exists for: without it, the whole value came back
    // on the header, in the error body, and on every log line it reached.
    // Truncation was rejected too, so nothing from the input may survive.
    let id = "a".repeat(1024 * 1024);
    let chosen = for_request(&headers_with(&id));

    assert!(uuid::Uuid::parse_str(&chosen).is_ok());
    assert!(!id.starts_with(&chosen));
}

#[test]
fn an_id_containing_a_control_character_is_replaced() {
    // A tab survives `HeaderValue` parsing and `to_str`, so the allowlist is
    // the only thing standing between it and a log field.
    assert!(was_generated(&headers_with("before\tafter")));
}

#[test]
fn an_id_containing_a_space_is_replaced() {
    assert!(was_generated(&headers_with("not an id")));
}

#[test]
fn an_id_containing_punctuation_no_id_format_uses_is_replaced() {
    for id in [
        "id\"quoted",
        "id{braced}",
        "id<angled>",
        "id;semicolon",
        "id,comma",
    ] {
        assert!(was_generated(&headers_with(id)), "{id} must be refused");
    }
}

#[test]
fn every_adopted_id_encodes_as_a_header_value() {
    // What lets `middleware` write the header unconditionally: an acceptable
    // id always encodes, so the fallback in `header_value` is unreachable.
    for id in ["plain", &"z".repeat(MAX_LENGTH), "a-b_c.d:e+f/g="] {
        assert!(acceptable(id));
        assert_eq!(header_value(id), id);
    }
}

#[test]
fn a_generated_id_also_encodes_as_a_header_value() {
    let generated = for_request(&HeaderMap::new());

    assert_eq!(header_value(&generated), generated);
}
