//! What the control plane needs to be told.

mod operator_config;
mod reconciliation_config;

pub use operator_config::OperatorConfig;
pub use reconciliation_config::ReconciliationConfig;

/// The control plane's configuration.
///
/// Deliberately small, and deliberately without a `Default`. The operator
/// posture has no safe default — every possible one either locks the platform
/// out or lets everybody in — so a deployment has to state it, and a missing
/// section is a startup failure rather than an inherited guess.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlPlaneConfig {
    /// How platform operators are authenticated.
    pub operator: OperatorConfig,

    /// How often reconciliation sweeps every client.
    #[serde(default)]
    pub reconciliation: ReconciliationConfig,
}
