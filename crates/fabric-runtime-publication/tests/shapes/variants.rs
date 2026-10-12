//! Every variant of an enum, enumerated by the enum's own serde derive.
//!
//! A hand-written list of variants beside an exhaustive `match` can drift:
//! a new variant gets its match arm and never reaches a specimen, so the
//! lock never sees what the writer emits. Here the names come from the
//! derive itself. Deserializing a name no variant has fails with every
//! variant's name ("expected one of ..."), and each name is then
//! deserialized into its variant, filling each required field the error
//! names with a specimen string, so a new variant is a specimen the moment
//! it exists.

use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};

/// A name no variant of any published enum has.
const NO_VARIANT: &str = "no-such-variant";

/// The value every required field of a variant is given.
const FIELD: &str = "specimen";

/// The variant names an error from the derive lists after "expected".
fn expected_names(error: &str) -> Vec<String> {
    let (_, expected) = error
        .split_once("expected")
        .unwrap_or_else(|| panic!("not an unknown-variant error: {error}"));
    expected
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// The field a "missing field `x`" error names.
fn missing_field(error: &str) -> Option<String> {
    let (_, rest) = error.split_once("missing field `")?;
    rest.split_once('`').map(|(field, _)| field.to_owned())
}

/// Every variant of a unit-variant enum serialized as a plain string.
pub fn every_unit<T: DeserializeOwned>() -> Vec<T> {
    let error = serde_json::from_value::<T>(json!(NO_VARIANT))
        .err()
        .unwrap()
        .to_string();
    let names = expected_names(&error);
    assert!(!names.is_empty(), "{error}");
    names
        .iter()
        .map(|name| serde_json::from_value(json!(name)).unwrap())
        .collect()
}

/// Every variant of an internally tagged enum whose tag is `tag`, each with
/// every required field set.
pub fn every_tagged<T: DeserializeOwned>(tag: &str) -> Vec<T> {
    let error = serde_json::from_value::<T>(json!({ tag: NO_VARIANT }))
        .err()
        .unwrap()
        .to_string();
    let names = expected_names(&error);
    assert!(!names.is_empty(), "{error}");
    names
        .iter()
        .map(|name| {
            let mut object = Map::new();
            object.insert(tag.to_owned(), json!(name));
            loop {
                match serde_json::from_value::<T>(Value::Object(object.clone())) {
                    Ok(variant) => break variant,
                    Err(error) => {
                        let error = error.to_string();
                        let field = missing_field(&error)
                            .unwrap_or_else(|| panic!("variant {name} cannot be built: {error}"));
                        assert!(
                            object.insert(field.clone(), json!(FIELD)).is_none(),
                            "variant {name}: {field} given and still missing"
                        );
                    }
                }
            }
        })
        .collect()
}
