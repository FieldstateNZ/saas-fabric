//! Connection pool sizing for a DataSource.

/// The pool configuration a DataSource declares (§22).
///
/// # Who acts on this
///
/// Not this process. Since data execution is delegated to connector processes
/// ([ADR 0001](../../../../docs/decisions/0001-ndc-as-connector-boundary.md)),
/// the pool lives inside the connector, and it is reconciliation's job to apply
/// these numbers to the connector's own configuration.
///
/// It is declared on the DataSource rather than left implicit in the
/// connector's config file because that is where it belongs conceptually: the
/// pool is a property of the *database being connected to*, sized against that
/// database's capacity. §22's objective — that connection count must not scale
/// with `replicas × tenants` — is a statement about a DataSource, and this is
/// where a reviewer can check it holds.
///
/// The runtime reports these values and does not interpret them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct PoolSettings {
    /// Maximum concurrent connections the connector may open to this
    /// DataSource, across all tenants bound to it.
    pub max_connections: u32,

    /// How long an idle connection is kept before eviction, in seconds.
    pub idle_timeout_seconds: u64,

    /// How long to wait for a connection before giving up, in seconds.
    pub acquire_timeout_seconds: u64,
}

impl Default for PoolSettings {
    fn default() -> Self {
        Self {
            max_connections: 20,
            idle_timeout_seconds: 300,
            acquire_timeout_seconds: 5,
        }
    }
}

impl PoolSettings {
    /// Checks the settings are usable.
    ///
    /// # Whose check this is
    ///
    /// **Reconciliation's, not the runtime's.** [`DataSource::validate`](crate::DataSource::validate) used
    /// to call this on every apply, so `max_connections: 0` kept a DataSource
    /// out of the registry entirely. That was disproportionate in the strict
    /// sense — the enforcement did far more damage than the fault it guarded
    /// against:
    ///
    /// - The runtime never reads these numbers. Execution is delegated to
    ///   connector processes (ADR 0001), so the pool lives in the connector,
    ///   and the type-level docs above say as much: the runtime reports these
    ///   values and does not interpret them. At the time of writing nothing in
    ///   the workspace consumes `max_connections` or `acquire_timeout_seconds`
    ///   at all — the only reference to a `DataSource`'s `pool` was this
    ///   method.
    /// - A refused DataSource is *absent*, so every tenant bound to it resolved
    ///   to [`ResolveError::MissingDataSource`](crate::ResolveError) — a 500,
    ///   and a misleading one, because the DataSource plainly does exist. A
    ///   resource that would have served every request correctly was withdrawn
    ///   over a field no request touches.
    /// - Rejecting did not even move the failure earlier, which is the usual
    ///   argument for validating at load. These numbers are applied by
    ///   reconciliation, so reconciliation is where they can be refused
    ///   *before* they become runtime state. Refusing them here converted a
    ///   future connector-sizing problem into a present-tense outage for
    ///   tenants who had nothing to do with the mistake.
    ///
    /// Dropping it also restores consistency with the rest of the crate, whose
    /// standing rule for a source that publishes something odd — see
    /// [`ApplyReport::divergent_payload`](crate::ApplyReport) — is to keep
    /// serving what works and say so loudly, not to withdraw it.
    ///
    /// The rule itself is kept and still tested, because it is a real rule; it
    /// simply belongs to whoever applies these numbers. One consequence worth
    /// naming rather than leaving implicit: nothing in *this* workspace calls
    /// this method any more, so until reconciliation does, an incoherent pool
    /// travels silently.
    ///
    /// # Errors
    ///
    /// Returns a message if the pool could never hand out a connection.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_connections == 0 {
            return Err("max_connections must be greater than zero".to_owned());
        }

        if self.acquire_timeout_seconds == 0 {
            return Err("acquire_timeout_seconds must be greater than zero".to_owned());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pool_that_can_never_hand_out_a_connection_is_rejected() {
        let settings = PoolSettings {
            max_connections: 0,
            ..PoolSettings::default()
        };

        assert!(settings.validate().is_err());
    }

    #[test]
    fn a_zero_acquire_timeout_is_rejected() {
        let settings = PoolSettings {
            acquire_timeout_seconds: 0,
            ..PoolSettings::default()
        };

        assert!(settings.validate().is_err());
    }

    #[test]
    fn the_defaults_are_usable() {
        assert!(PoolSettings::default().validate().is_ok());
    }
}
