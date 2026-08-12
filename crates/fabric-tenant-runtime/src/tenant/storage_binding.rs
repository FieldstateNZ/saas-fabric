//! Where one of a tenant's storage areas currently lives.

use fabric_connector::SecretRef;

/// The resolved location of one of a tenant's object-storage areas.
///
/// Like [`ConfigurationBinding`](crate::ConfigurationBinding), this is here
/// ahead of the Storage API (§27) so the binding format is stable. It follows
/// the same rule as everything else in a binding: a reference to a credential,
/// never a credential (§21).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageBinding {
    /// The storage endpoint or provider this area lives on.
    pub provider: String,

    /// The container, bucket, or share.
    pub container: String,

    /// An optional prefix scoping the tenant within a shared container.
    #[serde(default)]
    pub prefix: Option<String>,

    /// Where to find the credential for this area.
    #[serde(default)]
    pub credentials: Option<SecretRef>,
}
