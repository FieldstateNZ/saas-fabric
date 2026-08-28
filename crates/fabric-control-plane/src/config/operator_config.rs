//! The deployment's operator-authentication posture.

use std::sync::Arc;

use crate::operator::{KeyHolder, OidcOperators, OperatorAuthenticator, TrustedHeaderOperators};

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

    /// Verify a token the platform's own identity provider issued.
    ///
    /// The production posture. Authority is a realm role rather than a list of
    /// names here, so adding an operator is done where joiners and leavers are
    /// already handled instead of in a deployment change.
    Oidc {
        /// The issuer every accepted token must name, matched exactly.
        ///
        /// This is the realm's issuer URL, and it is also where the endpoints
        /// the console is sent to are derived from — so a deployment states it
        /// once rather than stating a set of URLs that could disagree.
        issuer: String,

        /// The client the console signs in as, and which an accepted token
        /// must have been issued to.
        #[serde(default = "default_client_id")]
        client_id: String,

        /// The realm role that confers operator authority.
        #[serde(default = "default_required_role")]
        required_role: String,

        /// Where the provider sends the browser back to after authenticating.
        ///
        /// The console's own origin. It is stated rather than derived from the
        /// request, because a redirect target taken from a `Host` header is a
        /// redirect target an attacker can choose.
        redirect_uri: String,

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

/// The client id the console signs in as.
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
    /// invalid header name, an empty allowlist, or a blank issuer, client or
    /// role.
    ///
    /// `keys` is the key set the OIDC posture verifies against, which the
    /// composition root owns because it also owns refreshing it. The
    /// trusted-header posture has no use for it and ignores it; that is
    /// preferable to two build functions, because it keeps every posture
    /// reachable from one match a reader can enumerate.
    pub fn build(&self, keys: Arc<KeyHolder>) -> Result<Box<dyn OperatorAuthenticator>, String> {
        match self {
            Self::TrustedHeader { header, allowlist } => {
                Ok(Box::new(TrustedHeaderOperators::new(header, allowlist)?))
            }

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
