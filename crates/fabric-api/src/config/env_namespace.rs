//! Which environment variables are settings, and which belong to the binary.
//!
//! # The namespace this used to share
//!
//! Every override was read from `FABRIC_*`, and [`AppConfig`](super::AppConfig)
//! denies unknown fields — so *any* `FABRIC_*` variable in the process
//! environment was parsed as a setting and anything unrecognised aborted
//! startup. Three entirely ordinary situations were fatal:
//!
//! | Variable | Set by | Read as |
//! |---|---|---|
//! | `FABRIC_CONFIG` | the documented invocation, and `main` itself | setting `config` |
//! | `FABRIC_SECRET_*` | [`EnvSecretResolver`](crate::secrets::EnvSecretResolver), the only documented way to supply a secret | setting `secret_…` |
//! | `FABRIC_API_PORT`, `FABRIC_API_SERVICE_HOST`, … | Kubernetes, for any Service named `fabric-*`, whenever `enableServiceLinks` is left at its default of true | setting `api_port` |
//!
//! Each error named a config key and pointed at the TOML file, so the
//! environment — the actual cause — was never implicated.
//!
//! # Why a narrower prefix, and not an exclusion list
//!
//! `deny_unknown_fields` is a genuine typo guard: `FABRIC_DATA_API__MAX_LIMITT`
//! and the single-underscore `FABRIC_DATA_API_MAX_LIMIT` both fail loudly
//! today, and that is worth keeping. So the fix cannot be to ignore unknown
//! keys.
//!
//! Excluding the names this binary owns would fix the first two rows and not
//! the third, because `FABRIC_API_PORT` is not a name this binary owns —
//! Kubernetes derives it from a *Service* name, so the set is unbounded and
//! unknowable here. Allowlisting the known top-level fields instead would
//! trade the typo guard away: `FABRIC_LISTENN` would be silently ignored, and
//! a Service named `fabric-data-api` would produce `FABRIC_DATA_API_PORT`,
//! which is indistinguishable from a real single-underscore mistake.
//!
//! Splitting the namespace is what leaves both properties intact.
//! `FABRIC_SETTING_` is exclusively for settings, so everything under it can
//! still be denied when unrecognised, while `FABRIC_CONFIG` and
//! `FABRIC_SECRET_*` sit outside it by construction rather than by a list
//! somebody has to maintain. A Kubernetes Service would have to be named
//! literally `fabric-setting` to collide, and if one ever were,
//! [`load_failure`](super::load_failure) now attributes the error to the
//! environment by name instead of blaming the file.

/// The prefix marking an environment variable as a setting.
///
/// Deliberately narrower than the process's general `FABRIC_` namespace — see
/// this module's documentation for what sharing it cost.
pub(super) const ENV_PREFIX: &str = "FABRIC_SETTING_";

/// The separator that maps a flat environment variable onto a nested field.
///
/// Two underscores rather than one, because field names contain single
/// underscores: `FABRIC_SETTING_DATA_API__MAX_LIMIT` is unambiguous where
/// `FABRIC_SETTING_DATA_API_MAX_LIMIT` could split three ways.
pub(super) const ENV_NESTING: &str = "__";

/// The variable naming the configuration file, read by `main`.
///
/// Exported so the binary and `ENV_PREFIX` cannot drift apart: the test
/// below asserts this sits outside the settings namespace, which is only a
/// real guarantee if `main` reads *this* constant rather than its own copy of
/// the string.
pub const CONFIG_PATH_VAR: &str = "FABRIC_CONFIG";

#[cfg(test)]
mod tests {
    use figment::providers::{Format as _, Toml};

    use super::*;
    use crate::config::AppConfig;
    use crate::secrets::EnvSecretResolver;

    #[test]
    fn the_config_path_variable_is_not_itself_a_setting() {
        assert!(
            !CONFIG_PATH_VAR.starts_with(ENV_PREFIX),
            "{CONFIG_PATH_VAR} would be parsed as a setting, so the documented invocation \
             could not start"
        );
    }

    #[test]
    fn the_secret_namespace_is_not_inside_the_settings_namespace() {
        assert!(
            !EnvSecretResolver::PREFIX.starts_with(ENV_PREFIX),
            "supplying a secret the only documented way would abort startup"
        );
    }

    #[test]
    fn kubernetes_service_links_are_not_parsed_as_settings() {
        // `enableServiceLinks` defaults to true, so a Service named `fabric-*`
        // injects these into every pod in the namespace. None is ours.
        for injected in [
            "FABRIC_API_PORT",
            "FABRIC_API_SERVICE_HOST",
            "FABRIC_API_SERVICE_PORT",
            "FABRIC_CONNECTOR_PORT_8080_TCP_ADDR",
        ] {
            assert!(
                !injected.starts_with(ENV_PREFIX),
                "{injected} would be parsed as a setting, so any Kubernetes deployment \
                 alongside a fabric-* Service could not start"
            );
        }
    }

    #[test]
    fn a_typo_in_a_real_setting_still_fails_loudly() {
        // The guard the narrower prefix had to preserve: denying unknown fields
        // is what turns a misspelled setting into a startup failure instead of
        // a value that silently does nothing.
        let result = figment::Figment::new()
            .merge(Toml::string("listenn = \"0.0.0.0:8080\""))
            .extract::<AppConfig>();

        assert!(result.is_err_and(|error| error.to_string().contains("listenn")));
    }
}
