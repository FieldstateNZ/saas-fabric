//! Reading configuration from a file and the environment.

use figment::providers::{Env, Format as _, Toml};
use figment::Figment;

use crate::config::env_namespace::{ENV_NESTING, ENV_PREFIX};
use crate::config::ControlPlaneAppConfig;

impl ControlPlaneAppConfig {
    /// Loads configuration from a file, then applies environment overrides.
    ///
    /// # Errors
    ///
    /// Returns a message if the file is missing or unreadable, or if the
    /// merged result does not deserialise — which includes an unknown key,
    /// because every config type sets `deny_unknown_fields`. A typo'd setting
    /// is a startup failure rather than a value that silently does nothing.
    pub fn load(path: &str) -> Result<Self, String> {
        require_readable_file(path)?;

        Figment::new()
            // `file_exact` rather than `file`: the latter walks *upward*
            // through parent directories, so a stray `control-plane.toml`
            // several levels above the working directory could be loaded in
            // place of the one that was asked for.
            .merge(Toml::file_exact(path))
            .merge(Env::prefixed(ENV_PREFIX).split(ENV_NESTING))
            .extract()
            .map_err(|error| format!("could not load control-plane configuration from {path}: {error}"))
    }
}

/// Refuses a configuration path that is not a readable file.
///
/// The runtime host learned this the hard way: `Toml::file_exact` treats a
/// missing file as an *empty provider*, so a mis-pathed `volumeMount` loaded
/// every default instead of failing. Here it would be worse — the operator
/// allowlist has no default at all, so the failure would be a confusing
/// missing-field error rather than a clear "your config file is not there".
fn require_readable_file(path: &str) -> Result<(), String> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(format!(
            "configuration path {path} is not a file; it cannot be loaded"
        )),
        Err(error) => Err(format!("could not read configuration file {path}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_fails_and_names_the_path() {
        let error = ControlPlaneAppConfig::load("/nonexistent/fabric/control-plane.toml");

        assert!(error.is_err_and(|message| message.contains("/nonexistent/fabric/control-plane.toml")));
    }

    #[test]
    fn a_directory_is_refused_rather_than_read_as_empty() {
        let error = ControlPlaneAppConfig::load(env!("CARGO_MANIFEST_DIR"));

        assert!(error.is_err_and(|message| message.contains("is not a file")));
    }
}
