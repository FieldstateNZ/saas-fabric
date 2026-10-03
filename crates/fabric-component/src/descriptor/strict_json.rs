//! A JSON walk that refuses a repeated key in any object.
//!
//! # Why this exists
//!
//! `serde_json` keeps the last of two equal keys and says nothing, so
//! `{"digest": "sha256:a…", "digest": "sha256:b…"}` reads as whichever came
//! second -- and a different parser, reading the same bytes, may keep the
//! first. A component descriptor names digests; two readers must never see
//! two documents in one. So it is refused, not resolved (ADR 0026 section 2).
//! `serde_json` bounds the nesting depth this recursion can reach.
use crate::errors::{invalid, ContractError};
use serde::de::{DeserializeSeed, Deserializer, Error, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeSet;
use std::fmt;

/// Refuses `text` if any object in it repeats a key, or it is not one JSON
/// value.
pub(crate) fn refuse_duplicate_keys(text: &str) -> Result<(), ContractError> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    Strict
        .deserialize(&mut deserializer)
        .and_then(|()| deserializer.end())
        .map_err(|error| invalid(format!("The component descriptor is not valid JSON: {error}")))
}

/// The seed and visitor for one value.
#[derive(Clone, Copy)]
struct Strict;

impl<'de> DeserializeSeed<'de> for Strict {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<(), A::Error> {
        while items.next_element_seed(self)?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<(), A::Error> {
        let mut seen = BTreeSet::new();
        while let Some(key) = entries.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(A::Error::custom(format!(
                    "the key {key:?} appears twice in one object"
                )));
            }
            entries.next_value_seed(self)?;
        }
        Ok(())
    }
}
