//! Where a published DataSource's data physically lives.

/// The publisher's own declaration of a DataSource's residency.
///
/// Mirrors `fabric_tenant_runtime::DataResidency` — see
/// [`crate::TenantBindingDocument`] for why this crate declares its own copy.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataResidencyDocument {
    /// The region this DataSource runs in, such as `au-east`.
    pub region: String,

    /// The legal jurisdiction governing the data, where it differs usefully
    /// from the region — `AU`, `EU`, `US`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<String>,
}
