//! Which environment variables are settings, and which belong to the binary.
//!
//! The same split the runtime host makes, and for the same reason its
//! `env_namespace` module records at length: a broad `FABRIC_*` namespace
//! collides with the config path variable, with every `FABRIC_SECRET_*` value
//! the platform projects into the pod, and with the `FABRIC_*_SERVICE_HOST`
//! variables Kubernetes injects for any Service named `fabric-*`. Each of
//! those was a startup failure that blamed the configuration file.
//!
//! `FABRIC_CP_SETTING_` is narrower still than the runtime's
//! `FABRIC_SETTING_`, because both processes run in the same namespace and a
//! shared settings namespace would let one process's override reach the other.

/// The prefix marking an environment variable as a control-plane setting.
pub(super) const ENV_PREFIX: &str = "FABRIC_CP_SETTING_";

/// The separator that maps a flat environment variable onto a nested field.
///
/// Two underscores rather than one, because field names contain single
/// underscores: `FABRIC_CP_SETTING_CONTROL_PLANE__RECONCILIATION__INTERVAL_SECONDS`
/// is unambiguous where a single underscore could split several ways.
pub(super) const ENV_NESTING: &str = "__";

/// The variable naming the configuration file, read by `main`.
pub const CONFIG_PATH_VAR: &str = "FABRIC_CP_CONFIG";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets;

    #[test]
    fn the_config_path_variable_is_not_itself_a_setting() {
        assert!(!CONFIG_PATH_VAR.starts_with(ENV_PREFIX));
    }

    #[test]
    fn the_secret_namespace_is_not_inside_the_settings_namespace() {
        // Supplying a secret the only documented way must not abort startup.
        assert!(!secrets::PREFIX.starts_with(ENV_PREFIX));
    }

    #[test]
    fn the_runtime_hosts_settings_do_not_reach_this_process() {
        // Both processes run in one namespace. A shared prefix would let a
        // runtime-plane override land in the control plane's configuration,
        // where `deny_unknown_fields` would abort startup for a reason nobody
        // could see.
        assert!(!"FABRIC_SETTING_LISTEN".starts_with(ENV_PREFIX));
    }

    #[test]
    fn kubernetes_service_links_are_not_parsed_as_settings() {
        for injected in [
            "FABRIC_CONTROL_PLANE_API_PORT",
            "FABRIC_CP_SERVICE_HOST",
            "FABRIC_API_SERVICE_PORT",
        ] {
            assert!(!injected.starts_with(ENV_PREFIX), "{injected} would be a setting");
        }
    }
}
