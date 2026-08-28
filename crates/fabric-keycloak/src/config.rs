//! What the Keycloak adapter needs to be told.

/// How to reach Keycloak, and as whom.
///
/// Every field here is non-secret and belongs in a `ConfigMap`. The secret
/// itself is named, never carried — see [`Self::client_secret_ref`].
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KeycloakConfig {
    /// The base URL of the Keycloak deployment, without a trailing slash.
    ///
    /// The **admin** address, which is not necessarily the one applications
    /// use: the platform deliberately exposes `/admin` only on the operator
    /// plane, so this is normally an in-cluster service address rather than
    /// the public `auth.<domain>`.
    pub base_url: String,

    /// The realm SaaS Fabric's own machine identity authenticates against.
    ///
    /// `master` by default, because that is where a realm-creating identity
    /// has to live: creating realms is a cross-realm operation, and no
    /// client-realm identity can perform it.
    pub admin_realm: String,

    /// The `client_id` of SaaS Fabric's machine identity.
    pub client_id: String,

    /// **The name of the secret**, not the secret.
    ///
    /// This application defines a configuration *contract* and nothing more:
    /// it says "there is a value called this, and I need it". How that value
    /// arrives — External Secrets projecting an OpenBao path into the pod's
    /// environment, a mounted file, something else entirely — is
    /// `saas-fabric-platform`'s decision (§20, §21), and encoding an opinion
    /// about it here would put a deployment mechanism in an application
    /// repository.
    ///
    /// The host resolves this reference and constructs an
    /// [`AdminCredential`](crate::AdminCredential); this crate never reads an
    /// environment variable or a file itself.
    pub client_secret_ref: String,

    /// How long a single admin API call may take, in seconds.
    ///
    /// Bounds the reconciliation sweep: a Keycloak that has stopped answering
    /// must fail the pass rather than hold it open, because a wedged sweep
    /// leaves every client's status frozen at whatever it last was.
    pub http_timeout_seconds: u64,
}

impl Default for KeycloakConfig {
    fn default() -> Self {
        Self {
            base_url: "http://keycloak-http.identity.svc.cluster.local".to_owned(),
            admin_realm: "master".to_owned(),
            client_id: "saas-fabric".to_owned(),
            client_secret_ref: "keycloak/saas-fabric".to_owned(),
            http_timeout_seconds: 10,
        }
    }
}

impl KeycloakConfig {
    /// Checks the settings that would otherwise fail on the first request.
    ///
    /// # Errors
    ///
    /// Returns a message if the base URL is not an absolute HTTP(S) URL, or if
    /// a required name is empty. Both are startup failures rather than
    /// per-request ones: a control plane that cannot reach its identity
    /// provider should say so before it starts sweeping.
    pub fn validate(&self) -> Result<(), String> {
        if !(self.base_url.starts_with("http://") || self.base_url.starts_with("https://")) {
            return Err(format!(
                "keycloak: base_url must be an absolute http(s) URL, got {}",
                self.base_url
            ));
        }

        for (name, value) in [
            ("admin_realm", &self.admin_realm),
            ("client_id", &self.client_id),
            ("client_secret_ref", &self.client_secret_ref),
        ] {
            if value.trim().is_empty() {
                return Err(format!("keycloak: {name} must not be empty"));
            }
        }

        Ok(())
    }
}
