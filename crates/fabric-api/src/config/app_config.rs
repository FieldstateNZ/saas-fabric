//! The shape of everything the runtime plane needs to start.
//!
//! The struct and its defaults only. Loading lives in
//! [`loading`](super::loading) and the cross-cutting checks in
//! [`validation`](super::validation) — three separable concerns that happen to
//! share a type.

use std::path::PathBuf;

use fabric_connector_ndc::NdcConnectorConfig;
use fabric_data_api::{DataApiConfig, ResourcePermissions};
use fabric_identity::IdentityConfig;
use fabric_tenant_runtime::RuntimeConfig;

use crate::config::TokenConfig;

/// The process's configuration, in one struct.
///
/// One type rather than several scattered lookups, so "what does this process
/// depend on?" is answered by reading one thing. Each domain's own config type
/// is nested rather than flattened, which keeps ownership of each setting with
/// the crate that understands it.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AppConfig {
    /// Address to bind, for example `0.0.0.0:8080`.
    pub listen: String,

    /// How the tenant identity context is derived.
    pub identity: IdentityConfig,

    /// Which token reader to use — the deployment's security posture.
    pub token: TokenConfig,

    /// How the runtime registries stay current.
    pub tenant_runtime: RuntimeConfig,

    /// The file reconciliation writes **tenant bindings** to.
    pub tenants_path: PathBuf,

    /// The file reconciliation writes **DataSources** to.
    ///
    /// Separate from the tenant file because the two are reconciled
    /// independently: resizing a pool rewrites this one and leaves tenant state
    /// untouched.
    pub data_sources_path: PathBuf,

    /// The resource catalogue file.
    pub catalog_path: PathBuf,

    /// Data API limits.
    pub data_api: DataApiConfig,

    /// Authorization defaults.
    pub permissions: ResourcePermissions,

    /// The connectors this process can execute against.
    pub connectors: Vec<NdcConnectorConfig>,
}

impl Default for AppConfig {
    /// The defaults a deployment inherits when it says nothing.
    ///
    /// Paths point at `/etc/fabric`, which is where a mounted `ConfigMap`
    /// conventionally lands. `connectors` is empty and
    /// [`validate`](super::validation) rejects that, so a process cannot start
    /// with nothing to execute against.
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:8080".to_owned(),
            identity: IdentityConfig::default(),
            token: TokenConfig::default(),
            tenant_runtime: RuntimeConfig::default(),
            tenants_path: PathBuf::from("/etc/fabric/tenants.json"),
            data_sources_path: PathBuf::from("/etc/fabric/data-sources.json"),
            catalog_path: PathBuf::from("/etc/fabric/catalog.json"),
            data_api: DataApiConfig::default(),
            permissions: ResourcePermissions::default(),
            connectors: Vec::new(),
        }
    }
}
