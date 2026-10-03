//! A component descriptor as a whole value inside another document: the
//! catalogue's frozen copy (ADR 0026 section 7).
//!
//! # Why the envelope travels with the copy
//!
//! A catalogue freezes the whole component descriptor it resolved --
//! `apiVersion` and `kind` included -- so every read validates each copy
//! with the rules of the version it records, and a copy of a version this
//! build does not read is refused, naming that version, rather than read as
//! a v1 document it is not. So these impls serialize exactly the envelope
//! [`to_json`](ComponentDescriptor::to_json) writes, and deserializing runs
//! the reader's own order: the envelope first, then the `spec`, which refuses
//! unknown fields, then [`validate`](ComponentDescriptor::validate).
//!
//! # Why the `spec` is buffered first
//!
//! The envelope is checked before the `spec` is parsed, so a later version
//! with fields v1 does not know is named as a later version, never as a v1
//! document with unknown fields. A deserializer hands a struct's fields over
//! in document order, so the `spec` is held as a [`serde_norway::Value`] --
//! a data-model value any format fills -- until the envelope is known.
//!
//! # Duplicate keys
//!
//! Not this impl's to check, and not left unchecked: the formats that carry a
//! copy refuse a repeated key before these impls see it. `serde_norway`
//! refuses one in any mapping, and a derived struct refuses a repeated
//! field; `frozen_tests.rs` pins both.
use super::{envelope, ComponentDescriptor, ComponentSpec};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

impl Serialize for ComponentDescriptor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        envelope::Written::wrapping(&self.spec).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ComponentDescriptor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let frozen = Frozen::deserialize(deserializer)?;
        envelope::check(frozen.api_version.as_deref(), frozen.kind.as_deref()).map_err(D::Error::custom)?;
        let spec = frozen
            .spec
            .ok_or_else(|| D::Error::missing_field("spec"))
            .and_then(|spec| ComponentSpec::deserialize(spec).map_err(D::Error::custom))?;
        Self::new(spec).map_err(D::Error::custom)
    }
}

/// The envelope as a copy is read: every part optional, so a missing one is
/// named by [`envelope::check`] rather than by a generic missing-field error.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Frozen {
    /// Checked before `spec` is parsed.
    #[serde(default)]
    api_version: Option<String>,
    /// As `api_version`.
    #[serde(default)]
    kind: Option<String>,
    /// The component itself, parsed once the envelope is known.
    #[serde(default)]
    spec: Option<serde_norway::Value>,
}
