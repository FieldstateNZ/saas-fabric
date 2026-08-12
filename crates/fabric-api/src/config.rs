//! The whole process's configuration, in one place.

use std::path::PathBuf;

use fabric_connector_ndc::NdcConnectorConfig;
use fabric_data_api::{DataApiConfig, ResourcePermissions};
use fabric_identity::IdentityConfig;
use fabric_tenant_runtime::TenantRuntimeConfig;
use figment::providers::{Env, Format as _, Toml};
use figment::Figment;

/// Everything the runtime plane needs to start.
///
/// One struct rather than several scattered lookups, so that "what does this
/// process depend on?" is answered by reading one type. Each domain's own
/// config type is nested rather than flattened, which keeps ownership of each
/// setting with the crate that understands it.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AppConfig {
    /// Address to bind, for example `0.0.0.0:8080`.
    pub listen: String,

    /// How the tenant identity context is derived.
    pub identity: IdentityConfig,

    /// Which token reader to use — the deployment's security posture.
    pub token: TokenConfig,

    /// How the tenant registry stays current.
    pub tenant_runtime: TenantRuntimeConfig,

    /// The file reconciliation writes tenant bindings to.
    pub bindings_path: PathBuf,

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
            tenant_runtime: TenantRuntimeConfig::default(),
            bindings_path: PathBuf::from("/etc/fabric/bindings.json"),
            catalog_path: PathBuf::from("/etc/fabric/catalog.json"),
            data_api: DataApiConfig::default(),
            permissions: ResourcePermissions::default(),
            connectors: Vec::new(),
        }
    }
}

/// How bearer tokens are read.
///
/// This is the single most consequential setting in the file, which is why it
/// is a tagged enum rather than a boolean buried in the identity section: a
/// reader glancing at the config should be able to see which posture is in
/// force without knowing what the flag means.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum TokenConfig {
    /// Verify token signatures against a JWKS document. **Recommended.**
    Validating {
        /// Path to a JWKS document, refreshed out of band.
        jwks_path: PathBuf,

        /// Accepted issuers. Strongly advised.
        #[serde(default)]
        issuers: Vec<String>,

        /// Accepted audiences.
        #[serde(default)]
        audiences: Vec<String>,
    },

    /// Trust the ingress and do not verify signatures.
    ///
    /// The §9 posture. Only sound while the network controls §9 also requires
    /// are actually in place — see `fabric_identity::TrustedIngressReader` for
    /// what goes wrong when they are not.
    TrustedIngress,
}

impl Default for TokenConfig {
    /// Defaults to the trusted-ingress posture *because it needs no key
    /// material to start*, which is the only reason. It is not the recommended
    /// production setting; `Validating` is.
    fn default() -> Self {
        Self::TrustedIngress
    }
}

impl AppConfig {
    /// Loads configuration from a file and the environment.
    ///
    /// Environment variables override the file and use `FABRIC_` with `__` for
    /// nesting: `FABRIC_DATA_API__MAX_LIMIT=200`.
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

        let mut seen = std::collections::BTreeSet::new();
        for connector in &self.connectors {
            if !seen.insert(connector.id.clone()) {
                return Err(format!("connector id {} is configured twice", connector.id));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_token_mode_is_trusted_ingress() {
        assert!(matches!(TokenConfig::default(), TokenConfig::TrustedIngress));
    }

    #[test]
    fn a_configuration_with_no_connectors_is_rejected() {
        assert!(AppConfig::default().validate().is_err());
    }

    #[test]
    fn duplicate_connector_ids_are_rejected() {
        let connector: NdcConnectorConfig =
            serde_json::from_str(r#"{"id":"postgres","endpoint":"http://a"}"#).unwrap();

        let config = AppConfig {
            connectors: vec![connector.clone(), connector],
            ..AppConfig::default()
        };

        assert!(config.validate().unwrap_err().contains("configured twice"));
    }

    #[test]
    fn a_single_connector_is_enough() {
        let connector: NdcConnectorConfig =
            serde_json::from_str(r#"{"id":"postgres","endpoint":"http://a"}"#).unwrap();

        let config = AppConfig {
            connectors: vec![connector],
            ..AppConfig::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn the_validating_token_mode_deserialises_from_its_tag() {
        let token: TokenConfig =
            serde_json::from_str(r#"{"mode":"validating","jwks_path":"/etc/jwks.json"}"#).unwrap();

        assert!(matches!(token, TokenConfig::Validating { .. }));
    }
}
