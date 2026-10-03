//! Application building blocks. Published snapshots never refer back to mutable drafts.
//!
//! One cohesive wire-format type: an application's definition, its
//! features, plans, navigation and published releases are the desired-state
//! document's shape for "what an application is", validated together in
//! `catalogue::validation::application`. A component is the one piece with a
//! file of its own, `component.rs`: since ADR 0026 it carries a resolution
//! and a frozen component descriptor, which is a shape of its own, not a
//! field of this one.
use super::{ApplicationComponent, ConfigurationField, ConfigurationValues};
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
    /// Per-client, non-secret configuration fields the operator authored.
    /// Every reader of a client's fields reads
    /// [`effective_fields`](Self::effective_fields) instead, which adds the
    /// ones each described component declares (ADR 0026 section 8).
    pub fields: Vec<ConfigurationField>,
    /// Client shell navigation, filtered by plan.
    pub navigation: Vec<NavigationItem>,
    /// Logical resources the operator authored for this application to
    /// expose through the Data API (ADR 0023 part 3). Every reader reads
    /// [`effective_resources`](Self::effective_resources) instead, which adds
    /// the ones each described component declares (ADR 0026 section 8).
    /// `#[serde(default)]` lets a release stored before
    /// this field existed parse unchanged; `skip_serializing_if` keeps it
    /// that way on the next render, rather than stamping `resources: []`
    /// onto a document this field did not touch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<super::ApplicationResource>,
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
