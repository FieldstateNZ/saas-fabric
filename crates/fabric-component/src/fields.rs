//! A typed, non-secret configuration field: the shape the catalogue's
//! authored fields, its `clientFields`, and a component descriptor's declared
//! fields all share.
//!
//! Moved here from `fabric-client-model` unchanged (ADR 0026 section 2) --
//! the same derives, serde attributes and field order, so a catalogue written
//! before the move reads and re-renders byte for byte. `fabric-client-model`
//! re-exports both types at their old paths.
use serde::{Deserialize, Serialize};

/// A typed configuration field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigurationField {
    /// Stable identifier, also the configuration map key.
    pub key: String,
    /// Display label.
    pub label: String,
    /// Input and validation rule.
    pub kind: FieldKind,
    /// Whether a value is mandatory.
    pub required: bool,
    /// Default used when no override is supplied.
    pub default: Option<String>,
    /// Allowed values for a choice field.
    pub options: Vec<String>,
    /// Help text.
    pub description: String,
}
/// Supported scalar input types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    /// Free text.
    Text,
    /// Finite decimal number.
    Number,
    /// True or false.
    Boolean,
    /// One of the field's declared options.
    Choice,
    /// A hostname.
    Hostname,
    /// A stable slug.
    Identifier,
    /// A timezone identifier.
    Timezone,
}
