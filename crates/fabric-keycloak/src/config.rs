//! What the Keycloak adapter needs to be told.
//!
//! In the 121–150 line band: one config type, its per-field serde defaults,
//! and its `validate()`. Splitting the default functions out from
//! `KeycloakConfig` would separate a type from the values it falls back to,
//! which is the field-level version of splitting a struct from its own impl.

/// How to reach Keycloak, and as whom.
///
/// Every field here is non-secret and belongs in a `ConfigMap`. The secret
/// itself is not here at all: the platform acts with an operator's bearer
/// rather than a credential of its own (ADR 0012).
///
/// `audience` carries no `#[serde(default)]`, unlike every other field here:
/// a document that omits it fails to deserialise rather than silently
/// inheriting a guessed value. See its own doc for why a default would be
/// actively dangerous rather than merely unhelpful.
///
/// **No `Default` impl, deliberately.** A guessed audience is exactly the
/// silent wrong value `audience`'s own doc refuses, and a `Default` on a
/// `pub` type is available to production code as much as to a test — nothing
/// stops `..KeycloakConfig::default()` reaching a composition root the day
/// someone finds it convenient. No code anywhere may construct one without
/// stating the audience by hand. A test that wants every other field at its
/// ordinary default still states this one explicitly, through a shared
/// helper (`config_for_tests` in this crate's `tests/support`) rather than
/// through this type.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeycloakConfig {
    /// The base URL of the Keycloak deployment, without a trailing slash.
    ///
    /// The **admin** address, which is not necessarily the one applications
    /// use: the platform deliberately exposes `/admin` only on the operator
    /// plane, so this is normally an in-cluster service address rather than
    /// the public `auth.<domain>`.
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// The realm SaaS Fabric's own machine identity authenticates against.
    ///
    /// `master` by default, because that is where a realm-creating identity
    /// has to live: creating realms is a cross-realm operation, and no
    /// client-realm identity can perform it.
    #[serde(default = "default_admin_realm")]
    pub admin_realm: String,

    /// The `client_id` of SaaS Fabric's machine identity.
    #[serde(default = "default_client_id")]
    pub client_id: String,

    /// How long a single admin API call may take, in seconds.
    ///
    /// Bounds a convergence pass: a Keycloak that has stopped answering must
    /// fail the pass rather than hold it open, because a wedged pass leaves
    /// every client's status frozen at whatever it last was — and now also
    /// holds an operator's request open while it does.
    #[serde(default = "default_http_timeout_seconds")]
    pub http_timeout_seconds: u64,

    /// The audience every declared public client's mapper asserts.
    ///
    /// **Required, with no default.** It **must equal the Data API's own
    /// required audience in this deployment**, which is in turn
    /// `IssuerRegistration.audience` for every issuer `fabric-fga-auth`
    /// registers in the same deployment (ADR 0019 §1 "The equality
    /// constraint", and §G5) — one API audience per deployment, and a client
    /// carries exactly one mapper. A silent default here (this field used to
    /// default to `saas-fabric-data-api`) would let a deployment start with a
    /// value nobody chose, and a mismatch does not fail loudly: it makes the
    /// edge refuse every genuine token from every client this adapter writes,
    /// presenting as a signature problem rather than a configuration one
    /// (ADR 0010). This crate cannot check the other side of that equality —
    /// it is a cross-crate, cross-deployment fact — so requiring the field is
    /// the one check available here: an operator who never set it gets a
    /// startup failure instead of a fleet of clients silently carrying the
    /// wrong audience.
    pub audience: String,
}

/// [`KeycloakConfig::base_url`]'s default: an in-cluster service address.
fn default_base_url() -> String {
    "http://keycloak-http.identity.svc.cluster.local".to_owned()
}

/// [`KeycloakConfig::admin_realm`]'s default: where a realm-creating identity
/// has to live.
fn default_admin_realm() -> String {
    "master".to_owned()
}

/// [`KeycloakConfig::client_id`]'s default.
fn default_client_id() -> String {
    "saas-fabric".to_owned()
}

/// [`KeycloakConfig::http_timeout_seconds`]'s default.
const fn default_http_timeout_seconds() -> u64 {
    10
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
            ("audience", &self.audience),
        ] {
            if value.trim().is_empty() {
                return Err(format!("keycloak: {name} must not be empty"));
            }
        }

        Ok(())
    }
}
