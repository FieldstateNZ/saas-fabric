//! How bearer tokens are read — the deployment's identity posture.

use std::path::PathBuf;

use crate::config::Allowlist;

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
    ///
    /// # Why `{}` and not a unit variant
    ///
    /// It carries no fields and reads as though it should be `TrustedIngress`,
    /// but `deny_unknown_fields` **does not bind on an internally-tagged unit
    /// variant**: serde matches the tag and discards the rest of the table
    /// without looking at it. A `[token]` section that named
    /// `mode = "trusted_ingress"` alongside `jwks_path`, `issuers` and
    /// `audiences` therefore loaded as this posture with no diagnostic at all —
    /// a deployment that had written down a validating configuration ran
    /// without one. The empty braces make this a struct variant, which is the
    /// shape `deny_unknown_fields` applies to, so those settings are now
    /// rejected by name.
    TrustedIngress {},

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

        /// Accepted issuers, or `None` to leave `iss` unexamined.
        ///
        /// Setting this makes the claim mandatory: a token omitting `iss` is
        /// refused rather than skipping the check. [`Allowlist`] is why there
        /// is no third state — an empty list is a startup failure, not a
        /// silent way back to `None`.
        #[serde(default)]
        issuers: Option<Allowlist>,

        /// Accepted audiences, or `None` to leave `aud` unexamined. The
        /// [`Self::Validating::issuers`] notes apply unchanged.
        #[serde(default)]
        audiences: Option<Allowlist>,
    },
}

impl Default for TokenConfig {
    /// Trusted ingress — the canonical posture, not merely the convenient one.
    fn default() -> Self {
        Self::TrustedIngress {}
    }
}

impl TokenConfig {
    /// A short name for startup logging.
    #[must_use]
    pub const fn mode_name(&self) -> &'static str {
        match self {
            Self::TrustedIngress {} => "trusted_ingress",
            Self::Validating { .. } => "validating",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_trusted_ingress() {
        assert!(matches!(TokenConfig::default(), TokenConfig::TrustedIngress {}));
    }

    #[test]
    fn the_validating_mode_deserialises_from_its_tag() {
        let token: TokenConfig =
            serde_json::from_str(r#"{"mode":"validating","jwks_path":"/etc/jwks.json"}"#).unwrap();

        assert!(matches!(token, TokenConfig::Validating { .. }));
    }

    #[test]
    fn each_mode_has_a_stable_name() {
        assert_eq!(TokenConfig::TrustedIngress {}.mode_name(), "trusted_ingress");
    }

    #[test]
    fn trusted_ingress_refuses_the_settings_only_the_other_posture_reads() {
        // The defect this pins: these three loaded silently, so a deployment
        // that had written down a validating posture ran without one.
        let result = serde_json::from_str::<TokenConfig>(
            r#"{"mode":"trusted_ingress","jwks_path":"/etc/jwks.json","issuers":["https://id.example.com"]}"#,
        );

        assert!(result.is_err_and(|error| error.to_string().contains("jwks_path")));
    }

    #[test]
    fn validating_still_refuses_an_unknown_setting() {
        let result =
            serde_json::from_str::<TokenConfig>(r#"{"mode":"validating","jwks_path":"/j","bogus":1}"#);

        assert!(result.is_err_and(|error| error.to_string().contains("bogus")));
    }

    #[test]
    fn an_empty_issuer_list_is_refused_rather_than_read_as_no_allowlist() {
        let result = serde_json::from_str::<TokenConfig>(
            r#"{"mode":"validating","jwks_path":"/etc/jwks.json","issuers":[]}"#,
        );

        assert!(result.is_err_and(|error| error.to_string().contains("Omit the setting")));
    }

    #[test]
    fn omitting_the_allowlists_leaves_both_claims_unexamined() {
        let token: TokenConfig =
            serde_json::from_str(r#"{"mode":"validating","jwks_path":"/etc/jwks.json"}"#).unwrap();

        assert!(matches!(
            token,
            TokenConfig::Validating {
                issuers: None,
                audiences: None,
                ..
            }
        ));
    }
}
