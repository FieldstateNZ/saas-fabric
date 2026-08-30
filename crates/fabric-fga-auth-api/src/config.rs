//! What this host needs to know, and what it refuses to start without.

use fabric_fga_auth::IssuerRegistration;
use figment::providers::{Env, Format as _, Toml};
use figment::Figment;
use serde::Deserialize;

/// Where the configuration file is named, when not given as an argument.
pub const CONFIG_PATH_VAR: &str = "FABRIC_FGA_CONFIG";

/// The whole configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    /// Where the runtime surface listens.
    ///
    /// The only address this image publishes. The authorization service it
    /// fronts is not reachable from outside the container at all — see
    /// [`Embedded`].
    pub listen: String,

    /// The authorization service inside this container.
    pub embedded: Embedded,

    /// Every issuer this deployment trusts.
    ///
    /// Never empty: a registry with no issuers authenticates nobody, and
    /// `Registry::build` refuses one.
    pub issuers: Vec<IssuerRegistration>,
}

/// The authorization service this process starts and supervises.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Embedded {
    /// The port it listens on, always on `127.0.0.1`.
    ///
    /// A port rather than an address, because there is no argument that should
    /// let it listen anywhere else. Nothing outside this container may reach
    /// it, which is what makes running it with no authentication of its own
    /// safe rather than reckless (ADR 0016).
    pub port: u16,

    /// The binary to run.
    pub binary: String,

    /// How long to wait for it to start answering before giving up.
    #[serde(default = "default_start_timeout")]
    pub start_timeout_seconds: u64,

    /// Where the authorization service keeps its data.
    ///
    /// **Absent means in memory, and in memory means every store, model and
    /// tuple is lost on restart.** That is a reasonable default for a
    /// development run and never for a deployment, so it is stated rather than
    /// assumed — and the host says so loudly at startup.
    #[serde(default)]
    pub datastore: Option<Datastore>,
}

/// Where the authorization service keeps its data.
///
/// `Debug` is written by hand and redacts the URI. The derive would have
/// printed a password the first time a configuration reached a log line, and
/// that is exactly how a connection string leaks — not by anybody deciding to
/// print it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Datastore {
    /// The engine, as the service names it — `postgres`, `mysql`.
    pub engine: String,

    /// The connection string.
    ///
    /// Carries a credential, so it is delivered as a secret and never appears
    /// in a rendered manifest. It is not logged here or anywhere else.
    pub uri: String,
}

/// Long enough for a cold start, short enough to fail a deployment.
const fn default_start_timeout() -> u64 {
    30
}

impl std::fmt::Debug for Datastore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Datastore")
            .field("engine", &self.engine)
            .field("uri", &"redacted")
            .finish()
    }
}

impl AppConfig {
    /// Loads configuration from a file, with environment overrides.
    ///
    /// # Errors
    ///
    /// Returns a message naming what could not be read or understood.
    pub fn load(path: &str) -> Result<Self, String> {
        Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed("FABRIC_FGA_SETTING_").split("__"))
            .extract()
            .map_err(|error| format!("{path}: {error}"))
    }
}
