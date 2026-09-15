//! Per-client product configuration and recorded changes.
use super::{ApplicationRelease, ConfigurationValues};
use crate::{ClientId, Host};
use serde::{Deserialize, Serialize};

/// The product section of a client document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientProduct {
    /// Commercial legal name.
    pub legal_name: String,
    /// Declared residency region.
    pub region: String,
    /// Client timezone.
    pub timezone: String,
    /// Shared contract version used at creation or update.
    pub definition_version: u32,
    /// Non-secret custom client configuration.
    pub configuration: ConfigurationValues,
    /// Application entitlements pinned to immutable snapshots.
    pub applications: Vec<ApplicationAssignment>,
    /// Changes to this client's product configuration.
    pub activity: Vec<ProductActivity>,
}
/// A client's entitlement to one published application version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationAssignment {
    /// Application key.
    pub application_id: ClientId,
    /// Complete snapshot, resolved by the server rather than supplied by the caller.
    pub release: ApplicationRelease,
    /// Selected plan key.
    pub plan_id: ClientId,
    /// Per-client configuration overrides.
    pub configuration: ConfigurationValues,
}
/// An assignment the operator requests; no caller-authored release snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignmentRequest {
    /// Application key.
    pub application_id: ClientId,
    /// Published definition version.
    pub version: u32,
    /// Plan key.
    pub plan_id: ClientId,
    /// Per-client values.
    pub configuration: ConfigurationValues,
}
/// Core and product fields an operator can edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientProductRequest {
    /// Client display name.
    pub display_name: String,
    /// Declared hostnames.
    pub hosts: Vec<Host>,
    /// Commercial legal name.
    pub legal_name: String,
    /// Residency region.
    pub region: String,
    /// Timezone identifier.
    pub timezone: String,
    /// Custom client values.
    pub configuration: ConfigurationValues,
    /// Applications to assign.
    pub applications: Vec<AssignmentRequest>,
}
/// New clients are created only when their identifier is unoccupied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateClientRequest {
    /// Permanent client identifier.
    pub id: ClientId,
    /// Initial configuration.
    pub configuration: ClientProductRequest,
}
/// Durable audit information stored alongside the desired-state edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductActivity {
    /// When the action was accepted, Unix seconds.
    pub at: u64,
    /// Authenticated operator subject.
    pub operator: String,
    /// Human-readable action.
    pub action: String,
    /// The resource the action concerns.
    pub resource: String,
}
