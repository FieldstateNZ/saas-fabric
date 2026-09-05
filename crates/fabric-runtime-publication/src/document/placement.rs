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
    fn every_variant_round_trips_through_the_runtimes_own_placement_class() {
        // Not just `HighAvailability` against itself -- every variant,
        // against the consumer's `fabric_tenant_runtime::PlacementClass`,
        // the same fidelity check every other document type gets. Compared
        // by re-serialising rather than `PlacementClass::as_str()`, which is
        // a telemetry label in a different convention (hyphens, not the
        // wire format's snake_case) and proves nothing about the wire shape.
        for document in [
            PlacementClassDocument::Shared,
            PlacementClassDocument::Dedicated,
            PlacementClassDocument::HighAvailability,
            PlacementClassDocument::Regulated,
            PlacementClassDocument::Development,
            PlacementClassDocument::Ephemeral,
        ] {
            let json = serde_json::to_string(&document).unwrap();

            let parsed: fabric_tenant_runtime::PlacementClass = serde_json::from_str(&json).unwrap();
            let round_tripped = serde_json::to_string(&parsed).unwrap();

            assert_eq!(round_tripped, json, "{document:?}");
        }
    }
}
