//! The service class a DataSource provides.

/// What kind of placement a DataSource represents (§17).
///
/// This is *descriptive metadata about the DataSource*, not a knob the runtime
/// interprets. §17 is explicit that placement policy is interpreted by the
/// platform — meaning reconciliation, which decides which DataSource a tenant
/// asking for `class: dedicated` should be bound to. By the time the runtime
/// sees a binding, that decision has been made.
///
/// It is recorded here so the runtime can report it (§29) and so an operator
/// can answer "which tenants are on regulated infrastructure?" without
/// inspecting servers.
///
/// **Applications must not depend on this** (§17). It never crosses the Data
/// API boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementClass {
    /// Shared with other tenants.
    Shared,
    /// Dedicated to one tenant.
    Dedicated,
    /// Replicated or clustered for availability.
    HighAvailability,
    /// Subject to regulatory constraints.
    Regulated,
    /// Non-production.
    Development,
    /// Short-lived; may be destroyed without notice.
    Ephemeral,
}

impl PlacementClass {
    /// A stable name for telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Dedicated => "dedicated",
            Self::HighAvailability => "high-availability",
            Self::Regulated => "regulated",
            Self::Development => "development",
            Self::Ephemeral => "ephemeral",
        }
    }

    /// Whether this class is intended to carry production data.
    ///
    /// Reported at startup so that a development or ephemeral DataSource
    /// appearing in a production deployment is visible in the logs rather than
    /// discovered later.
    #[must_use]
    pub const fn is_production(self) -> bool {
        matches!(
            self,
            Self::Shared | Self::Dedicated | Self::HighAvailability | Self::Regulated
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialises_from_snake_case() {
        let class: PlacementClass = serde_json::from_str(r#""high_availability""#).unwrap();

        assert_eq!(class, PlacementClass::HighAvailability);
    }

    #[test]
    fn development_and_ephemeral_are_not_production_classes() {
        assert!(!PlacementClass::Development.is_production());
        assert!(!PlacementClass::Ephemeral.is_production());
        assert!(PlacementClass::Regulated.is_production());
    }
}
