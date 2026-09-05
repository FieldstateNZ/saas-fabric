//! Where one of a tenant's storage areas currently lives, as the publisher
//! declares it.

/// The publisher's own declaration of one tenant storage area.
///
/// Mirrors `fabric_tenant_runtime::StorageBinding` — see
/// [`TenantBindingDocument`](crate::TenantBindingDocument) for why this crate
/// declares its own copy. `credentials` is a reference **path** only, never a
/// resolved value: nothing in this crate is handed a secret resolver, and it
/// depends on no crate that has one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageBindingDocument {
    /// The storage endpoint or provider this area lives on.
    pub provider: String,

    /// The container, bucket, or share.
    pub container: String,

    /// An optional prefix scoping the tenant within a shared container.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,

    /// Where to find the credential for this area — a reference path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials: Option<String>,
}
