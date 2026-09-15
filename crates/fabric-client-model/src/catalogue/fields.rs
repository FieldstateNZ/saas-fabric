//! Non-secret configuration schemas and values.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Scalar values are submitted as text and validated against the field's type.
pub type ConfigurationValues = BTreeMap<String, String>;
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
/// Console-wide presentation settings and new-client defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConsoleSettings {
    /// Platform display name.
    pub platform_name: String,
    /// Default region for newly created clients.
    pub default_region: String,
    /// Timezone used for client defaults.
    pub timezone: String,
}
impl Default for ConsoleSettings {
    fn default() -> Self {
        Self {
            platform_name: "SaaS Fabric".into(),
            default_region: "New Zealand".into(),
            timezone: "Pacific/Auckland".into(),
        }
    }
}
/// A link to an independently managed environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentRegistration {
    /// Stable key.
    pub id: crate::ClientId,
    /// Display name.
    pub name: String,
    /// Operator console URL; credentials are never stored here.
    pub console_url: String,
    /// Operator-facing description.
    pub description: String,
}
