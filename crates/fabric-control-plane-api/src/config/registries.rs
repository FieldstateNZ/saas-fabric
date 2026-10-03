//! The image registries operators register: how long a call to one may take.

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
}

impl Default for RegistriesConfig {
    fn default() -> Self {
        Self {
            http_timeout_seconds: default_timeout(),
        }
    }
}

/// Ten seconds, matching the other platform clients.
const fn default_timeout() -> u64 {
    10
}
