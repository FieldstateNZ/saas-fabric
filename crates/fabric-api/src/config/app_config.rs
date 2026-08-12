//! Everything the runtime plane needs to start.

use std::collections::BTreeSet;
use std::path::PathBuf;

use fabric_connector_ndc::NdcConnectorConfig;
use fabric_data_api::{DataApiConfig, ResourcePermissions};
use fabric_identity::IdentityConfig;
use fabric_tenant_runtime::RuntimeConfig;
use figment::providers::{Env, Format as _, Toml};
use figment::Figment;

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

impl AppConfig {
    /// Loads configuration from a file and the environment.
    ///
    /// Environment variables override the file, prefixed `FABRIC_` with `__`
    /// for nesting: `FABRIC_DATA_API__MAX_LIMIT=200`.
    ///
    /// # Errors
    ///
    /// Returns a message if the file cannot be read or the result does not
    /// deserialise.
    pub fn load(path: &str) -> Result<Self, String> {
        Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed("FABRIC_").split("__"))
            .extract()
            .map_err(|error| format!("could not load configuration from {path}: {error}"))
    }

    /// Checks settings that span domains.
    ///
    /// Each domain validates its own settings when it is built. What is left
    /// for here is the cross-cutting cases no single domain can see.
    ///
    /// # Errors
    ///
    /// Returns a message describing the first problem found.
    pub fn validate(&self) -> Result<(), String> {
        if self.connectors.is_empty() {
            return Err("at least one connector must be configured".to_owned());
        }

        let mut seen = BTreeSet::new();
        for connector in &self.connectors {
            if !seen.insert(connector.id.clone()) {
                return Err(format!("connector id {} is configured twice", connector.id));
            }

            connector.validate()?;
        }

        if self.tenants_path == self.data_sources_path {
            return Err(
                "tenants_path and data_sources_path must differ: the two resources are reconciled \
                 independently and cannot share a file"
                    .to_owned(),
            );
        }

        Ok(())
    }
}
