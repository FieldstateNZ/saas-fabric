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
    /// Where this control plane is reachable from an operator's browser.
    ///
    /// The origin the console and this API share behind the operator-plane
    /// ingress — for example `https://fabric.example.test`.
    ///
    /// **Stated, never derived from a request.** Every use of it is a URL a
    /// browser will be sent to, or one a Git host will send a browser to, and
    /// a redirect target taken from a `Host` header is a redirect target
    /// whoever made the request chose.
    ///
    /// Empty when a deployment states none, which is legal: only the Git
    /// connection flow needs it, and a deployment not using that flow should
    /// not have to state a URL to start.
    #[serde(default)]
    pub public_base_url: String,

    /// How platform operators are authenticated.
    pub operator: OperatorConfig,

    /// How often reconciliation sweeps every client.
    #[serde(default)]
    pub reconciliation: ReconciliationConfig,
}
