//! Whether, and how often, this deployment publishes the runtime's three
//! documents.

/// Runtime publication's configuration, nested under
/// `[platform_management.publication]`.
///
/// Absent (the default, via `PlatformManagementConfig.publication`) means
/// **deliberately unconfigured**: a deployment that manages a platform
/// repository but publishes no runtime state — `GET /api/platform` reports
/// no publication row, and the trigger answers `PublicationNotConfigured`.
/// That is different from configured-and-broken, which is a startup
/// failure — see `startup::platform`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationConfig {
    /// The Kubernetes namespace the runtime's three `ConfigMap`s are
    /// written to.
    ///
    /// Validated at startup against
    /// `fabric_publication_kubernetes::PublicationTarget::validate` — the
    /// adapter's own rule, not a second copy of it — so a namespace that is
    /// not a DNS label fails startup rather than every publication pass.
    pub namespace: String,

    /// How often the schedule runs.
    ///
    /// **Zero disables the schedule and keeps the trigger working** — the
    /// same shape `PlatformManagementConfig::reconciliation_interval_seconds`
    /// gives the sweep, for the same reason: a deployment that wants to
    /// publish only when an operator asks says so here, without also
    /// losing `POST /api/platform/publication`.
    #[serde(default = "default_interval")]
    pub interval_seconds: u64,
}

/// A minute — the same default the platform sweep uses, and for the same
/// reason: short enough that an integration environment feels immediate,
/// long enough that the publication target is not asked constantly.
const fn default_interval() -> u64 {
    60
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: serde_json::Value) -> Result<PublicationConfig, serde_json::Error> {
        serde_json::from_value(value)
    }

    #[test]
    fn a_namespace_alone_parses_with_the_default_interval() {
        let config = parse(serde_json::json!({ "namespace": "platform-system" }))
            .expect("namespace alone must be enough");

        assert_eq!(config.namespace, "platform-system");
        assert_eq!(config.interval_seconds, 60);
    }

    #[test]
    fn a_stated_interval_overrides_the_default() {
        let config = parse(serde_json::json!({
            "namespace": "platform-system",
            "interval_seconds": 30,
        }))
        .expect("both fields must parse");

        assert_eq!(config.interval_seconds, 30);
    }

    #[test]
    fn an_unknown_field_is_refused_rather_than_ignored() {
        let error = parse(serde_json::json!({
            "namespace": "platform-system",
            "typo_field": true,
        }))
        .expect_err("an unrecognised field must not be silently dropped");

        assert!(error.to_string().contains("typo_field"), "{error}");
    }

    #[test]
    fn a_missing_namespace_is_refused() {
        assert!(parse(serde_json::json!({})).is_err());
    }
}
