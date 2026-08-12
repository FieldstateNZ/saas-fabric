//! Where a tenant's configuration currently lives.

/// The resolved location of a tenant's configuration store.
///
/// Corresponds to `config → appconfig/acme` in §7. The Configuration API is a
/// future sibling of the Data API (§27); this type exists now so the binding
/// shape does not have to change when it arrives, and so a tenant definition
/// can already carry the information.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationBinding {
    /// The configuration store this tenant reads from.
    pub store: String,

    /// The tenant's profile within that store, such as `enterprise`.
    #[serde(default)]
    pub profile: Option<String>,
}
