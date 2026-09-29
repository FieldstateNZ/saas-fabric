//! Non-secret configuration schemas and values.
//!
//! [`ConfigurationField`] and [`FieldKind`] are declared in
//! `fabric-component`, beside the component descriptor that also declares
//! them (ADR 0026 section 2), and re-exported here unchanged -- including as
//! the type of the catalogue's `clientFields`, which is not about components
//! at all. One declaration of the shape, not two.
pub use fabric_component::{ConfigurationField, FieldKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Scalar values are submitted as text and validated against the field's type.
pub type ConfigurationValues = BTreeMap<String, String>;
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
