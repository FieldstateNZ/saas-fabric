//! Where a tenant's configuration currently lives, as the publisher declares
//! it.

/// The publisher's own declaration of a tenant's configuration location.
///
/// Mirrors `fabric_tenant_runtime::ConfigurationBinding` — see
/// [`TenantBindingDocument`](crate::TenantBindingDocument) for why this crate
/// declares its own copy rather than depending on the crate that owns the
/// original.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationBindingDocument {
    /// The configuration store this tenant reads from.
    pub store: String,

    /// The tenant's profile within that store, such as `enterprise`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}
