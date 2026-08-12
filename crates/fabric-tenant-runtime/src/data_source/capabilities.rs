//! What the platform permits a DataSource to be used for.

/// Platform-level policy on a DataSource.
///
/// # Two switches, two different audiences
///
/// | Field | Who reads it | When it applies |
/// |---|---|---|
/// | [`writable`](Self::writable) | The **runtime**, on every write | Per request, before the connector is called |
/// | [`accepts_new_tenants`](Self::accepts_new_tenants) | **Reconciliation**, when placing a tenant | Never on the request path |
///
/// Conflating them would be a serious bug: draining a DataSource must not break
/// the tenants already on it.
///
/// # Both default to `false`
///
/// A DataSource must *declare* what it permits. The alternative — defaulting to
/// permissive — means a reconciler that forgets `writable: false` on a read
/// replica sends writes to it, and one that forgets `accepts_new_tenants:
/// false` on a draining source keeps receiving tenants. Both failures are
/// silent at the point of the mistake and expensive later.
///
/// Failing closed costs one line per DataSource and makes the intent explicit
/// in the reconciled state, where it can be reviewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct DataSourceCapabilities {
    /// Whether SaaS Fabric permits write operations against this DataSource.
    ///
    /// This is **platform policy**, distinct from whether the connector
    /// technically supports mutations
    /// ([`ConnectorCapabilities::mutations`](fabric_connector::ConnectorCapabilities)).
    /// Both must be true before a write is attempted, and the check here runs
    /// first — before the connector is called at all.
    ///
    /// A read replica is the motivating case: its connector can express writes
    /// perfectly well, and the replica would reject them at some depth with a
    /// vendor-specific error. Declaring `writable: false` refuses the write
    /// with a clear message and no wasted round trip.
    pub writable: bool,

    /// Whether reconciliation may bind **new** tenants to this DataSource.
    ///
    /// Purely control-plane. The runtime request path never reads it, and must
    /// never start: setting it to `false` drains a DataSource by stopping new
    /// placement, and tenants already bound to it keep working exactly as
    /// before.
    ///
    /// If a DataSource genuinely needs to stop serving existing traffic, that
    /// is a different state and needs to be modelled separately — do not
    /// overload this one.
    pub accepts_new_tenants: bool,
}

impl Default for DataSourceCapabilities {
    /// Fail closed: a DataSource permits nothing it has not declared.
    fn default() -> Self {
        Self {
            writable: false,
            accepts_new_tenants: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undeclared_data_source_permits_nothing() {
        // A reconciler that forgets the block gets the safe answer, not the
        // convenient one.
        let capabilities = DataSourceCapabilities::default();

        assert!(!capabilities.writable);
        assert!(!capabilities.accepts_new_tenants);
    }

    #[test]
    fn an_empty_capability_block_is_still_fail_closed() {
        let capabilities: DataSourceCapabilities = serde_json::from_str("{}").unwrap();

        assert!(!capabilities.writable);
    }

    #[test]
    fn the_two_switches_are_independent() {
        // Draining must not make a DataSource read-only for the tenants on it.
        let draining: DataSourceCapabilities =
            serde_json::from_str(r#"{"writable": true, "accepts_new_tenants": false}"#).unwrap();

        assert!(draining.writable);
        assert!(!draining.accepts_new_tenants);
    }

    #[test]
    fn a_read_replica_can_still_accept_new_tenants() {
        let replica: DataSourceCapabilities =
            serde_json::from_str(r#"{"writable": false, "accepts_new_tenants": true}"#).unwrap();

        assert!(!replica.writable);
        assert!(replica.accepts_new_tenants);
    }

    #[test]
    fn an_unknown_capability_is_rejected_rather_than_ignored() {
        // A typo like "writeable" must not silently leave the DataSource
        // read-only while the operator believes they enabled writes.
        assert!(serde_json::from_str::<DataSourceCapabilities>(r#"{"writeable": true}"#).is_err());
    }
}
