//! Application building blocks. Published snapshots never refer back to mutable drafts.
use super::{ConfigurationField, ConfigurationValues};
use crate::ClientId;
use serde::{Deserialize, Serialize};

/// One product application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Application {
    /// Stable application identifier.
    pub id: ClientId,
    /// Editable next definition.
    pub draft: ApplicationDefinition,
    /// Immutable published definitions, oldest first.
    pub releases: Vec<ApplicationRelease>,
}
/// Everything needed to determine a client's application entitlement.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationDefinition {
    /// Display name.
    pub name: String,
    /// Product description.
    pub description: String,
    /// Optional hostname template containing `{client}`.
    pub domain: String,
    /// Deployables and platform capabilities.
    pub components: Vec<ApplicationComponent>,
    /// Product features and their implementation dependencies.
    pub features: Vec<ApplicationFeature>,
    /// Plans granting features.
    pub plans: Vec<ApplicationPlan>,
    /// Per-client, non-secret configuration fields.
    pub fields: Vec<ConfigurationField>,
    /// Client shell navigation, filtered by plan.
    pub navigation: Vec<NavigationItem>,
}
/// A deployable or platform capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationComponent {
    /// Stable key within its application.
    pub id: ClientId,
    /// Display name.
    pub name: String,
    /// Deployment mechanism.
    pub kind: ComponentKind,
    /// OCI image, chart reference, or capability name.
    pub reference: String,
    /// Pinned artifact version or digest; empty for platform capabilities.
    pub version: String,
    /// Included for every plan.
    pub required: bool,
    /// Automatic or manual advancement intent.
    pub policy: UpdatePolicy,
}
/// Supported component categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentKind {
    /// Container image.
    Container,
    /// Chart installation.
    Helm,
    /// Platform-provided service.
    Capability,
}
/// How a deployable may advance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePolicy {
    /// Follow the platform's approved releases.
    Automatic,
    /// Require an operator's decision.
    Manual,
}
/// A feature implemented by one or more components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationFeature {
    /// Stable feature key.
    pub id: ClientId,
    /// Display name.
    pub name: String,
    /// Operator-facing description.
    pub description: String,
    /// Component keys this feature needs.
    pub implemented_by: Vec<ClientId>,
}
/// A named entitlement set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationPlan {
    /// Stable plan key.
    pub id: ClientId,
    /// Display name.
    pub name: String,
    /// Short description.
    pub description: String,
    /// Granted feature keys.
    pub features: Vec<ClientId>,
    /// Non-secret plan limits and settings.
    pub configuration: ConfigurationValues,
}
/// A client-facing navigation item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigationItem {
    /// Display label.
    pub label: String,
    /// Same-origin route, never an external or script URL.
    pub route: String,
    /// Optional feature controlling visibility.
    pub feature: Option<ClientId>,
    /// Permission identifier for the application to enforce.
    pub permission: String,
}
/// Immutable application definition at publication time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationRelease {
    /// Monotonically increasing definition version.
    pub version: u32,
    /// Operator's release note.
    pub note: String,
    /// Publication time, Unix seconds.
    pub published_at: u64,
    /// Complete definition snapshot.
    pub definition: ApplicationDefinition,
}
