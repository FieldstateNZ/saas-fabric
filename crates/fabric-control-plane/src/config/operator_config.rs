//! The deployment's operator-authentication posture.

use crate::operator::{OperatorAuthenticator, TrustedHeaderOperators};

/// How this deployment establishes who an operator is.
///
/// # A tagged enum, so the posture is stated rather than assembled
///
/// A flat struct with an optional header and an optional allowlist would let a
/// deployment end up in a state nobody chose — a header configured and an
/// allowlist forgotten, say. Naming the mode makes the posture one decision,
/// visible in a diff, and makes adding a second one an additive change rather
/// than a reinterpretation of the fields already there.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperatorConfig {
    /// Consume an identity established by the operator-plane proxy.
    ///
    /// The only posture this increment implements. See
    /// [`TrustedHeaderOperators`] for what makes it safe and what makes it
    /// unsafe.
    TrustedHeader {
        /// The header the proxy states the operator's identity in.
        ///
        /// Defaults to Tailscale's, because that is the operator plane this
        /// platform runs today — but it is configurable, because the header is
        /// a property of whatever proxy is actually in front of the service,
        /// and hard-coding it would silently authenticate nobody behind a
        /// different one.
        #[serde(default = "default_header")]
        header: String,

        /// The operators permitted to administer this platform.
        ///
        /// May not be empty. See
        /// [`TrustedHeaderOperators`](crate::TrustedHeaderOperators).
        allowlist: Vec<String>,
    },
}

/// Tailscale's identity header.
fn default_header() -> String {
    "Tailscale-User-Login".to_owned()
}

impl OperatorConfig {
    /// Builds the authenticator this posture describes.
    ///
    /// # Errors
    ///
    /// Returns a message if the posture is not usable as configured — an
    /// invalid header name, or an empty allowlist.
    pub fn build(&self) -> Result<Box<dyn OperatorAuthenticator>, String> {
        match self {
            Self::TrustedHeader { header, allowlist } => {
                Ok(Box::new(TrustedHeaderOperators::new(header, allowlist)?))
            }
        }
    }
}
