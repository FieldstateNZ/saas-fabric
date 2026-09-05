//! The service class a published DataSource provides.

/// The publisher's own declaration of a DataSource's placement class.
///
/// Mirrors `fabric_tenant_runtime::PlacementClass` — see
/// [`crate::TenantBindingDocument`] for why this crate declares its own copy.
/// Descriptive metadata only: nothing in this crate branches on it, exactly
/// as nothing in the runtime's request path does either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementClassDocument {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialises_from_snake_case() {
        let class: PlacementClassDocument = serde_json::from_str(r#""high_availability""#).unwrap();

        assert_eq!(class, PlacementClassDocument::HighAvailability);
    }
}
