//! Reading configuration from a file and the environment.

use figment::providers::{Env, Format as _, Toml};
use figment::Figment;

use crate::config::env_namespace::{ENV_NESTING, ENV_PREFIX};
use crate::config::{load_failure, AppConfig};

impl AppConfig {
    /// Loads configuration from a file, then applies environment overrides.
    ///
    /// Environment wins over the file, so a deployment can override one setting
    /// without templating the whole document. Settings are namespaced
    /// `FABRIC_SETTING_` and nesting uses a double underscore:
    /// `FABRIC_SETTING_DATA_API__MAX_LIMIT=200` sets `data_api.max_limit`. See
    /// `env_namespace` for why that prefix is narrower
    /// than the process's general `FABRIC_` one.
    ///
    /// # Errors
    ///
    /// Returns a message if the file is missing or unreadable, or if the merged
    /// result does not deserialise — which includes an unknown key, because
    /// every config type sets `deny_unknown_fields`. A typo'd setting is a
    /// startup failure rather than a value that silently does nothing, and
    /// `load_failure` names whichever of the two sources
    /// was actually at fault.
    pub fn load(path: &str) -> Result<Self, String> {
        require_readable_file(path)?;

        Figment::new()
            // `file_exact` rather than `file`: the latter walks *upward* through
            // parent directories looking for a relative path, so a stray
            // `config.toml` several levels above the working directory could be
            // loaded in place of the one that was asked for.
            .merge(Toml::file_exact(path))
            .merge(Env::prefixed(ENV_PREFIX).split(ENV_NESTING))
            .extract()
            .map_err(|error| load_failure::describe(&error, path))
    }
}

/// Refuses a configuration path that is not a readable file.
///
/// # Why this is checked rather than left to figment
///
/// `Toml::file_exact` treats a missing file as an *empty provider*, so a
/// mis-pathed `volumeMount` or an unmounted `ConfigMap` did not fail: the
/// process loaded every default instead. That is the worst possible reading of
/// a missing file, because the defaults include the identity posture — a
/// deployment that had configured `token` reverted to trusted ingress on the
/// strength of a typo in a path, and the only symptom was a later, unrelated
/// complaint about connectors.
///
/// Failing here is safe even for a deployment that configures almost
/// everything through the environment: `connectors` is a table array that no
/// single environment variable can express, and [`AppConfig::validate`]
/// already requires at least one. A file is effectively mandatory either way,
/// so requiring it to exist takes nothing away.
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
        let error = AppConfig::load("/nonexistent/fabric/config.toml");

        assert!(error.is_err_and(|message| message.contains("/nonexistent/fabric/config.toml")));
    }

    #[test]
    fn a_missing_file_does_not_silently_become_the_defaults() {
        // The defect this pins: loading used to succeed here, reverting every
        // setting — including the identity posture — to its default.
        assert!(AppConfig::load("/nonexistent/fabric/config.toml").is_err());
    }

    #[test]
    fn a_directory_is_refused_rather_than_read_as_empty() {
        // A `volumeMount` that lands a ConfigMap as a directory reaches here.
        let error = AppConfig::load(env!("CARGO_MANIFEST_DIR"));

        assert!(error.is_err_and(|message| message.contains("is not a file")));
    }
}
