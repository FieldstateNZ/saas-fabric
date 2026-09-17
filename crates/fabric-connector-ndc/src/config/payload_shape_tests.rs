//! Tests for `payload_shape`.

use super::payload_shape::*;

#[test]
fn the_default_shape_is_values() {
    assert_eq!(PayloadShape::default(), PayloadShape::Values);
}

#[test]
fn parses_snake_case_from_configuration() {
    let shape: PayloadShape = serde_json::from_str(r#""set_operations""#).unwrap();

    assert_eq!(shape, PayloadShape::SetOperations);
}
