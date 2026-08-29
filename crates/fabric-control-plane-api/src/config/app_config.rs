//! The shape of everything the control plane needs to start.

use fabric_control_plane::ControlPlaneConfig;

use crate::config::{DesiredStateConfig, GitHostConfig, IdentityProviderConfig, SecretStoreConfig};

/// The process's configuration, in one struct.
///
/// # No `Default`, on purpose
///
/// Three of these four fields have no safe default. The operator posture has
/// none — every possible one either locks the platform out or lets everybody
/// in. The desired-state repository has none, because a default that quietly
/// became an empty in-memory store would present a platform with clients as a
/// platform with none. The identity provider has none, because a default that
/// reconciled nothing would show every client as converged.
///
/// So a deployment states all three, and a missing section is a startup
/// failure rather than an inherited guess. The runtime host can afford
/// `#[serde(default)]` because its defaults are paths and timeouts; this one
/// cannot.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlPlaneAppConfig {
    /// Address to bind, for example `0.0.0.0:8081`.
    ///
    /// A different port from the runtime API's by convention, because the two
    /// run side by side in development and on entirely different networks in
    /// production.
    pub listen: String,

    /// The control plane's own settings: operator posture, sweep interval.
    pub control_plane: ControlPlaneConfig,

    /// Where desired state lives.
    pub desired_state: DesiredStateConfig,

    /// Which identity provider reconciliation converges.
    pub identity_provider: IdentityProviderConfig,

    /// Where this instance keeps its own durable state.
    ///
    /// Defaulted, unlike the three above, because a deployment that does not
    /// connect its own integration never touches it — and the default is the
    /// development one, which says so loudly at startup.
    #[serde(default)]
    pub secret_store: SecretStoreConfig,

    /// The Git host the platform creates its application on.
    #[serde(default)]
    pub git_host: GitHostConfig,

    /// The overall budget for one control-plane request, in seconds.
    ///
    /// Bounds a handler that is waiting on the Git host. Without it, a
    /// repository that accepts connections and never answers would hold
    /// operator requests open until the browser gave up, with nothing in the
    /// logs to say why.
    #[serde(default = "default_request_timeout")]
    pub request_timeout_seconds: u64,
}

/// Twice the Git adapter's own default call timeout, so the adapter's timeout
/// is the one that fires and produces the clearer failure.
const fn default_request_timeout() -> u64 {
    30
}
