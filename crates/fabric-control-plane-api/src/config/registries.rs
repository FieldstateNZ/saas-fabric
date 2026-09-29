//! The image registries operators register: how long a call to one may
//! take, and how long resolving a selected version may.

/// `[registries]`: a deployment's settings for the registries operators
/// register (ADR 0026 section 5).
///
/// # Why a section of its own
///
/// Registries and the version picker exist whether or not Platform Management
/// is configured, so their settings cannot live under
/// `[platform_management]`. Which registries exist is not here at all: that
/// is operator-managed state, registered in the console. The deployment's
/// *own* registry keeps its section and its timeout,
/// `[platform_management.registry]`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistriesConfig {
    /// How long one call to an operator-registered registry may take.
    ///
    /// Zero is refused at startup: `reqwest` reads it as no timeout, and a
    /// registry that accepted a connection and never answered would hold an
    /// operator's change open for ever.
    #[serde(default = "default_timeout")]
    pub http_timeout_seconds: u64,

    /// How long resolving one selected component version may take, every
    /// registry read included (ADR 0026 section 7). At the deadline the reads
    /// still in flight are abandoned — reads are safe to abandon — and the
    /// operator is told the registry could not be asked.
    ///
    /// Zero is refused at startup, and so is a budget that does not fit in
    /// one request between a Git read and a Git write:
    /// `git_host.http_timeout_seconds` plus this plus
    /// `git_host.http_timeout_seconds` must be below
    /// `request_timeout_seconds`.
    #[serde(default = "default_resolution_budget")]
    pub resolution_budget_seconds: u64,
}

impl Default for RegistriesConfig {
    fn default() -> Self {
        Self {
            http_timeout_seconds: default_timeout(),
            resolution_budget_seconds: default_resolution_budget(),
        }
    }
}

/// Eight seconds: with the Git host's default ten on either side, a
/// selection fits the default thirty-second request with two to spare.
const fn default_resolution_budget() -> u64 {
    8
}

/// Ten seconds, matching the other platform clients.
const fn default_timeout() -> u64 {
    10
}
