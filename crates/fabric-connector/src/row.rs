//! A single record moving in either direction across the boundary.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::FieldName;

/// One record: field names to JSON values.
///
/// `BTreeMap` rather than `HashMap` so field order is deterministic. That makes
/// generated queries stable, responses byte-comparable in tests, and diffs
/// readable — small things that each save an afternoon eventually.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Row(BTreeMap<FieldName, Value>);

impl Row {
    /// An empty row.
    #[must_use]
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Adds or replaces a field, returning the row for chaining.
    #[must_use]
    pub fn with(mut self, field: FieldName, value: Value) -> Self {
        self.0.insert(field, value);
        self
    }

    /// Reads a field.
    #[must_use]
    pub fn get(&self, field: &FieldName) -> Option<&Value> {
        self.0.get(field)
    }

    /// The field names present on this row.
    pub fn fields(&self) -> impl Iterator<Item = &FieldName> {
        self.0.keys()
    }

    /// How many fields the row carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the row carries no fields.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Borrows the underlying map.
    #[must_use]
    pub const fn as_map(&self) -> &BTreeMap<FieldName, Value> {
        &self.0
    }
}

impl From<BTreeMap<FieldName, Value>> for Row {
    fn from(fields: BTreeMap<FieldName, Value>) -> Self {
        Self(fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_iterate_in_a_deterministic_order() {
        let row = Row::new()
            .with(FieldName::try_new("zebra").unwrap(), Value::Null)
            .with(FieldName::try_new("apple").unwrap(), Value::Null);

        let names: Vec<&str> = row.fields().map(FieldName::as_str).collect();
        assert_eq!(names, ["apple", "zebra"]);
    }
}
