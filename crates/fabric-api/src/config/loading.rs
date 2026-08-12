//! Reading configuration from a file and the environment.

use figment::providers::{Env, Format as _, Toml};
use figment::Figment;

use crate::config::AppConfig;

/// The environment prefix every override carries.
const ENV_PREFIX: &str = "FABRIC_";

/// The separator that maps a flat environment variable onto a nested field.
const ENV_NESTING: &str = "__";

impl AppConfig {
    /// Loads configuration from a file, then applies environment overrides.
    ///
    /// Environment wins over the file, so a deployment can override one setting
    /// without templating the whole document. Nesting uses a double underscore:
    /// `FABRIC_DATA_API__MAX_LIMIT=200` sets `data_api.max_limit`.
    ///
    /// # Errors
    ///
    /// Returns a message if the file cannot be read, or if the merged result
    /// does not deserialise — which includes an unknown key, because every
    /// config type sets `deny_unknown_fields`. A typo'd setting is a startup
    /// failure rather than a value that silently does nothing.
    pub fn load(path: &str) -> Result<Self, String> {
        Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed(ENV_PREFIX).split(ENV_NESTING))
            .extract()
            .map_err(|error| format!("could not load configuration from {path}: {error}"))
    }
}
