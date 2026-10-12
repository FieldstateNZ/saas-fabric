//! A published document's shape, read from the bytes the publisher writes.
//!
//! One line per fact a reader depends on:
//! - every field, after serde renames, and its JSON kind;
//! - the variants of a tagged enum, each with its own fields;
//! - the values of a plain string enum;
//! - every field that some instance leaves out (`may be absent`).
//!
//! Values are not shape: an id, a revision or a label never appears. Which
//! paths are maps, tagged enums and string enums is stated per document in
//! [`Rules`], because nothing in the JSON says so.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// How to read one document's JSON.
pub struct Rules {
    /// Objects whose keys are data (ids, names), not fields.
    pub maps: &'static [&'static str],
    /// Internally tagged enums: the path and its tag field.
    pub tagged: &'static [(&'static str, &'static str)],
    /// Strings whose value is an enum variant.
    pub enums: &'static [&'static str],
}

/// The shape of one document, as sorted lines.
pub fn shape(document: &Value, rules: &Rules) -> BTreeSet<String> {
    let mut walk = Walk {
        rules,
        lines: BTreeSet::new(),
        fields: BTreeMap::new(),
    };
    walk.value(document, "$");
    let mut lines = walk.lines;
    for (path, (union, every)) in walk.fields {
        for field in union.difference(&every) {
            lines.insert(format!("{path}.{field}: may be absent"));
        }
    }
    lines
}

struct Walk<'a> {
    rules: &'a Rules,
    lines: BTreeSet<String>,
    /// Per object path: every field seen, and the fields every instance had.
    fields: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
}

impl Walk<'_> {
    fn line(&mut self, path: &str, kind: &str) {
        self.lines.insert(format!("{path}: {kind}"));
    }

    fn value(&mut self, value: &Value, path: &str) {
        match value {
            Value::Object(object) => self.object(object, path),
            Value::Array(items) => {
                self.line(path, "array");
                for item in items {
                    self.value(item, &format!("{path}[]"));
                }
            }
            Value::String(text) if self.rules.enums.contains(&path) => {
                self.line(path, &format!("one of {text:?}"));
            }
            Value::String(_) => self.line(path, "string"),
            Value::Number(number) if number.is_f64() => self.line(path, "number"),
            Value::Number(_) => self.line(path, "integer"),
            Value::Bool(_) => self.line(path, "boolean"),
            Value::Null => self.line(path, "null"),
        }
    }

    fn object(&mut self, object: &serde_json::Map<String, Value>, path: &str) {
        if self.rules.maps.contains(&path) {
            self.line(path, "map");
            for entry in object.values() {
                self.value(entry, &format!("{path}.*"));
            }
            return;
        }
        let tag = self
            .rules
            .tagged
            .iter()
            .find(|(tagged, _)| *tagged == path)
            .map(|(_, tag)| *tag);
        let path = match tag {
            Some(tag) => {
                self.line(path, &format!("object tagged by {tag}"));
                let variant = object
                    .get(tag)
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| panic!("{path} is not tagged by {tag}: the tag changed"));
                format!("{path}<{tag}={variant}>")
            }
            None => path.to_owned(),
        };
        self.line(&path, "object");
        let present: BTreeSet<String> = object
            .keys()
            .filter(|key| Some(key.as_str()) != tag)
            .cloned()
            .collect();
        let (union, every) = self
            .fields
            .entry(path.clone())
            .or_insert_with(|| (BTreeSet::new(), present.clone()));
        union.extend(present.iter().cloned());
        every.retain(|field| present.contains(field));
        for field in &present {
            self.value(&object[field], &format!("{path}.{field}"));
        }
    }
}
