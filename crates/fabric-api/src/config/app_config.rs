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

    /// How often the background retry loop attempts to renegotiate a
    /// connector that failed at startup, in seconds (§35).
    ///
    /// Deliberately its own setting rather than reusing
    /// [`RuntimeConfig::refresh_interval_seconds`]: that one bounds staleness
    /// for two *local* file reads, while this one bounds how quickly the
    /// platform notices a *remote* connector process has come back after
    /// failing to negotiate. Different failure mode, different cost — this
    /// one is an HTTP round trip per still-failed connector, per tick — so it
    /// gets a knob of its own rather than silently inheriting one tuned for a
    /// different job.
    pub connector_retry_interval_seconds: u64,

    /// The overall budget for one Data API request, in seconds.
    ///
    /// This is the **outermost** of three timeout scopes in this system, and
    /// the only one this process owns directly:
    ///
    /// | Scope | Owned by | This example's default |
    /// |---|---|---|
    /// | Overall request budget | **here** — `AppConfig::request_timeout_seconds` | 30s |
    /// | HTTP call to the connector | `fabric-connector-ndc`'s per-connector [`NdcConnectorConfig::http_timeout_seconds`] | 10s |
    /// | Database execution inside the connector | the connector process's own configuration | not visible to SaaS Fabric at all |
    ///
    /// The second is a different crate's setting, configured per connector
    /// instance in `[[connectors]]` — see that field's own docs for the full
    /// three-clock breakdown from the connector's side. The third never
    /// appears in SaaS Fabric configuration: once a query reaches the
    /// connector process, how long the database may run it is that
    /// process's business.
    ///
    /// **Must not be shorter than the longest configured connector
    /// timeout.** If it were, this budget would always expire before a slow
    /// connector's own timeout could ever fire, making the connector's
    /// setting a dead letter and turning a slow-but-legitimate request into
    /// an unexplained cutoff instead of the connector's own, clearer
    /// failure. [`AppConfig::validate`] enforces this relationship at
    /// startup rather than leaving it to be discovered under load.
    pub request_timeout_seconds: u64,
}

impl Default for AppConfig {
    /// The defaults a deployment inherits when it says nothing.
    ///
    /// Paths point at `/etc/fabric`, which is where a mounted `ConfigMap`
    /// conventionally lands. `connectors` is empty and
    /// `validation::validate` rejects that, so a process cannot start
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
            connector_retry_interval_seconds: 30,
            // 3x the connector default of 10s: enough headroom that raising a
            // connector's own timeout a little never silently breaks this
            // relationship, while still bounding the worst case.
            request_timeout_seconds: 30,
        }
    }
}
