//! How bearer tokens are read — the deployment's identity posture.

use std::path::PathBuf;

/// Which token reader the process runs.
///
/// A tagged enum rather than a flag buried in the identity section, so the
/// deployed posture is legible at a glance. See
/// [ADR 0002](../../../../docs/decisions/0002-trusted-ingress-is-the-canonical-identity-model.md).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum TokenConfig {
    /// Consume the identity the platform edge established. **The default and
    /// the canonical architecture** (§8, §9).
    ///
    /// The runtime parses claims and checks expiry. It does not re-validate
    /// what the gateway validated, and takes on no identity-provider
    /// responsibilities (§24).
    ///
    /// This depends on the other half of §9: protected runtime APIs must not be
    /// reachable through an untrusted path.
    TrustedIngress,

    /// Additionally verify token signatures. Optional defence in depth.
    ///
    /// Not the recommended architecture, and not a substitute for the network
    /// policy §9 requires — if untrusted callers can reach the runtime
    /// directly, that is the thing to fix. Use where a second layer is
    /// genuinely wanted, such as a regulated environment expecting
    /// verification at more than one hop.
    Validating {
        /// Path to a JWKS document, loaded once at startup. Never fetched.
        jwks_path: PathBuf,

        /// Accepted issuers.
        #[serde(default)]
        issuers: Vec<String>,

        /// Accepted audiences.
        #[serde(default)]
        audiences: Vec<String>,
    },
}

impl Default for TokenConfig {
    /// Trusted ingress — the canonical posture, not merely the convenient one.
    fn default() -> Self {
        Self::TrustedIngress
    }
}

impl TokenConfig {
    /// A short name for startup logging.
    #[must_use]
    pub const fn mode_name(&self) -> &'static str {
        match self {
            Self::TrustedIngress => "trusted_ingress",
            Self::Validating { .. } => "validating",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_trusted_ingress() {
        assert!(matches!(TokenConfig::default(), TokenConfig::TrustedIngress));
    }

    #[test]
    fn the_validating_mode_deserialises_from_its_tag() {
        let token: TokenConfig =
            serde_json::from_str(r#"{"mode":"validating","jwks_path":"/etc/jwks.json"}"#).unwrap();

        assert!(matches!(token, TokenConfig::Validating { .. }));
    }

    #[test]
    fn each_mode_has_a_stable_name() {
        assert_eq!(TokenConfig::TrustedIngress.mode_name(), "trusted_ingress");
    }
}
