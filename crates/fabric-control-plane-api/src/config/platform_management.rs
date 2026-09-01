//! Which platform repository this deployment manages, and how often.

/// Platform Management's configuration.
///
/// Absent means **deliberately unconfigured**: a deployment that manages no
/// platform repository. That is different from configured-and-broken, which is
/// a startup failure — see `startup::platform`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformManagementConfig {
    /// Which repository, and which environment within it.
    pub repository: PlatformRepositoryBinding,

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

    /// The secret holding the credential for the platform repository.
    ///
    /// # Interim
    ///
    /// The specification puts this credential behind a Platform Management
    /// GitHub App an operator installs, separate from the Client Configuration
    /// one. That flow does not exist yet, so the credential is stated by the
    /// deployment for now — the same shape every other platform credential
    /// has, resolved through the environment.
    ///
    /// When the App flow lands, this field is what it replaces.
    pub credential: String,
}

/// Which repository, and which environment within it.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformRepositoryBinding {
    /// The account it belongs to.
    pub owner: String,

    /// Its name.
    pub name: String,

    /// The branch the environment follows.
    #[serde(default = "default_branch")]
    pub branch: String,

    /// Which environment in that repository this deployment manages.
    ///
    /// One, deliberately. A control plane manages the environment it is
    /// deployed into; managing somebody else's from here would mean a single
    /// process advancing a cluster it cannot see.
    pub environment: String,

    /// Where the host's API lives.
    #[serde(default = "default_api_base_url")]
    pub api_base_url: String,

    /// How long a call to it may take.
    #[serde(default = "default_timeout")]
    pub http_timeout_seconds: u64,
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

/// The branch every environment follows by default.
fn default_branch() -> String {
    "main".to_owned()
}

/// GitHub's API.
fn default_api_base_url() -> String {
    "https://api.github.com".to_owned()
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
