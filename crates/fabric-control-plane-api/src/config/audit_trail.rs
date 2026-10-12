//! Where the durable audit trail is stored, if anywhere (ADR 0028).

/// The audit trail's storage.
///
/// # Disabled by default, on purpose
///
/// Where the trail is stored in a live environment, and for how long, are
/// decisions deferred by ADR 0028: storage activation and retention are not
/// approved with the design. So a deployment that states nothing gets
/// `disabled` — audit records reach the log pipeline exactly as they did
/// before the trail existed, and startup says so — and the only storage on
/// offer is a local development directory.
///
/// A tagged enum for the same reason
/// [`DesiredStateConfig`](super::DesiredStateConfig) is one: the modes are not
/// interchangeable, and choosing one is written down.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuditTrailConfig {
    /// No durable trail: records reach the log pipeline only.
    #[default]
    Disabled,

    /// An append-only trail in a local directory. **Development only** —
    /// startup logs a warning saying so. See
    /// [`LocalAuditTrail`](crate::local_audit_trail::LocalAuditTrail).
    LocalDirectory {
        /// The directory holding one log and one head per stream.
        path: std::path::PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unstated_trail_is_disabled() {
        assert!(matches!(AuditTrailConfig::default(), AuditTrailConfig::Disabled));
    }

    #[test]
    fn a_local_directory_is_stated_by_its_mode_and_path() {
        let config: AuditTrailConfig =
            serde_json::from_value(serde_json::json!({"mode": "local_directory", "path": ".local/audit"}))
                .unwrap();

        assert!(matches!(config, AuditTrailConfig::LocalDirectory { path } if path.ends_with("audit")));
    }

    #[test]
    fn a_mode_that_does_not_exist_is_refused() {
        let refused = serde_json::from_value::<AuditTrailConfig>(serde_json::json!({"mode": "git"}));

        assert!(refused.is_err());
    }
}
