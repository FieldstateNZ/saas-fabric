//! Which platform repository this deployment manages, and how often.

/// Platform Management's configuration.
///
/// Absent means **deliberately unconfigured**: a deployment that manages no
/// platform repository. That is different from configured-and-broken, which is
/// a startup failure — see `startup::platform`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformManagementConfig {
    /// Which environment in the platform repository this deployment is.
    ///
    /// The one thing here a deployment genuinely knows. *Which* repository,
    /// and the credential for it, are operator-managed integration state: an
    /// operator installs the Platform Management GitHub App and picks a
    /// repository, exactly as they do for client configuration.
    pub environment: String,

    /// Where published artifacts are looked up.
    #[serde(default)]
    pub registry: RegistryBinding,

    /// How often the sweep runs.
    ///
    /// **Zero disables it**, and so does omitting the whole section. A
    /// deployment that wants to observe an environment without advancing it
    /// says so here rather than in a build.
    ///
    /// No environment name appears in this binary. `LucentRoot`'s deployment
    /// supplies sixty because it is an integration environment; something
    /// slower elsewhere is a change to a manifest, not to a release.
    #[serde(default = "default_interval")]
    pub reconciliation_interval_seconds: u64,
}

/// Where published artifacts are looked up.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryBinding {
    /// Where to talk to the registry.
    #[serde(default = "default_registry_base_url")]
    pub base_url: String,

    /// How repositories are *named*, which is not always where they are served
    /// from. A manifest says `ghcr.io/…` whatever endpoint this is pointed at.
    #[serde(default = "default_registry_host")]
    pub host: String,

    /// How long a call to it may take.
    #[serde(default = "default_timeout")]
    pub http_timeout_seconds: u64,
}

impl Default for RegistryBinding {
    fn default() -> Self {
        Self {
            base_url: default_registry_base_url(),
            host: default_registry_host(),
            http_timeout_seconds: default_timeout(),
        }
    }
}

/// A minute. Short enough that an integration environment feels immediate,
/// long enough that a registry is not being asked constantly.
const fn default_interval() -> u64 {
    60
}

/// Where SaaS Fabric's own images are published.
fn default_registry_base_url() -> String {
    "https://ghcr.io".to_owned()
}

/// And how they are named.
fn default_registry_host() -> String {
    "ghcr.io".to_owned()
}

/// Ten seconds, matching the other platform clients.
const fn default_timeout() -> u64 {
    10
}
