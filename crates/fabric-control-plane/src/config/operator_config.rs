//! The deployment's operator-authentication posture.

use std::sync::Arc;

use crate::operator::{KeyHolder, OidcOperators, OperatorAuthenticator};

/// How this deployment establishes who an operator is.
///
/// # One variant, and the tag stays anyway
///
/// There is exactly one posture, so this could be a plain struct. It keeps the
/// `mode` tag because a deployment stating its authentication posture out loud
/// is worth more than the line it costs, and because the last time this had
/// two variants one of them was safe only by accident of where it ran. If a
/// second is ever added it will be an additive change rather than a
/// reinterpretation of fields already here.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperatorConfig {
    /// Verify a token the platform's own identity provider issued.
    ///
    /// The only posture. Authority is a realm role rather than a list of names
    /// here, so adding an operator is done where joiners and leavers are
    /// already handled instead of in a deployment change — and the bearer the
    /// operator presents is what the platform then acts with (ADR 0012).
    Oidc {
        /// The issuer every accepted token must name, matched exactly.
        ///
        /// This is the realm's issuer URL — the realm the gateway signs
        /// operators in to (ADR 0024 §1). It is also where the signing keys are
        /// read from unless `reachable_at` says otherwise.
        issuer: String,

        /// Where **this process** reaches the same realm.
        ///
        /// Defaults to the issuer, which is right whenever one URL serves
        /// both. On a cluster it often does not: the issuer is a public
        /// hostname that only resolves outside, and the pod has a service
        /// address that only resolves inside. The signing keys are fetched
        /// from here; a token still has to name the issuer.
        ///
        /// Getting this wrong fails in a way that reads as something else —
        /// every operator refused, and a log saying either that no key set
        /// arrived or that the token is not from this realm.
        #[serde(default)]
        reachable_at: String,

        /// The client an accepted token must have been issued to.
        ///
        /// Behind the gateway (ADR 0024) that is the gateway's own client,
        /// because the gateway runs the sign-in and forwards the token it was
        /// issued; the console signs in as nothing.
        #[serde(default = "default_client_id")]
        client_id: String,

        /// The realm role that confers operator authority.
        #[serde(default = "default_required_role")]
        required_role: String,

        /// Clock skew tolerated on `exp` and `nbf`, in seconds.
        #[serde(default = "default_leeway")]
        leeway_seconds: u64,

        /// How often the provider's signing keys are re-read, in seconds.
        ///
        /// Bounds how long a rotation takes to be noticed. Five minutes is
        /// short next to how often a realm rotates keys and long next to how
        /// often it is asked; the cost of being wrong in one direction is a
        /// window of refused sign-ins, and in the other a request nobody
        /// needed.
        #[serde(default = "default_refresh")]
        jwks_refresh_seconds: u64,
    },
}

/// The client id an accepted token must have been issued to, when a
/// deployment states none.
fn default_client_id() -> String {
    "saas-fabric-console".to_owned()
}

/// The realm role that makes somebody an operator.
fn default_required_role() -> String {
    "fabric-operator".to_owned()
}

/// How often signing keys are re-read.
fn default_refresh() -> u64 {
    300
}

/// Tolerated clock skew. A minute: enough for hosts that disagree slightly,
/// short enough that an expired token is not usable for meaningfully longer
/// than it was issued for.
fn default_leeway() -> u64 {
    60
}

impl OperatorConfig {
    /// The realm role this posture requires, as the authenticator it builds
    /// holds it: trimmed, the way [`OidcOperators::new`] stores it.
    #[must_use]
    pub fn required_role(&self) -> &str {
        match self {
            Self::Oidc { required_role, .. } => required_role.trim(),
        }
    }

    /// Builds the authenticator this posture describes.
    ///
    /// # Errors
    ///
    /// Returns a message if the posture is not usable as configured — a blank
    /// issuer, client or role.
    ///
    /// `keys` is the key set the OIDC posture verifies against, which the
    /// composition root owns because it also owns refreshing it.
    pub fn build(&self, keys: Arc<KeyHolder>) -> Result<Box<dyn OperatorAuthenticator>, String> {
        match self {
            Self::Oidc {
                issuer,
                client_id,
                required_role,
                leeway_seconds,
                ..
            } => Ok(Box::new(OidcOperators::new(
                issuer,
                client_id,
                required_role,
                keys,
                *leeway_seconds,
            )?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OperatorConfig;

    fn posture(required_role: Option<&str>) -> OperatorConfig {
        let mut value = serde_json::json!({
            "mode": "oidc",
            "issuer": "https://auth.example.test/realms/master",
        });
        if let Some(role) = required_role {
            value["required_role"] = role.into();
        }
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn the_required_role_defaults_to_the_operator_role() {
        assert_eq!(posture(None).required_role(), "fabric-operator");
    }

    #[test]
    fn the_required_role_is_what_the_deployment_states_trimmed() {
        assert_eq!(
            posture(Some(" platform-admin ")).required_role(),
            "platform-admin"
        );
    }
}
