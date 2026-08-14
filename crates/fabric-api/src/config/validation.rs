//! Checks that span more than one domain.

use std::collections::BTreeSet;

use crate::config::{administrator_role, AppConfig};

impl AppConfig {
    /// Checks the settings no single domain can see.
    ///
    /// Each domain validates its own settings when it is built — connector
    /// timeouts, page-size ceilings, refresh intervals. What is left for here
    /// is the cross-cutting cases: relationships *between* settings, and
    /// between settings owned by different crates.
    ///
    /// # Errors
    ///
    /// Returns a message describing the first problem found.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_connectors()?;
        self.validate_state_paths()?;
        administrator_role::validate(&self.permissions)?;
        self.validate_timeouts()
    }

    /// Requires at least one connector, all distinct and individually valid.
    ///
    /// A process with no connectors can execute nothing, and a duplicate id
    /// means one connector silently replaces another in the registry — a
    /// misconfiguration that would otherwise show up as requests going to the
    /// wrong database.
    fn validate_connectors(&self) -> Result<(), String> {
        if self.connectors.is_empty() {
            return Err("at least one connector must be configured".to_owned());
        }

        let mut seen = BTreeSet::new();

        for connector in &self.connectors {
            if !seen.insert(connector.id.clone()) {
                return Err(format!("connector id {} is configured twice", connector.id));
            }

            connector.validate()?;
        }

        Ok(())
    }

    /// Requires the timeout settings this struct owns directly to be usable,
    /// and to sit in the right relationship to each other (§36).
    ///
    /// `request_timeout_seconds` is the outermost of three timeout scopes —
    /// see its rustdoc — and must never be shorter than the longest
    /// configured connector timeout. If it were, the overall budget would
    /// always cut a request off before that connector's own timeout could
    /// fire, making the connector's setting unreachable and turning a
    /// slow-but-legitimate request into an unexplained cutoff instead of the
    /// connector's own clearer failure.
    ///
    /// `connector_retry_interval_seconds` just needs to be positive, for the
    /// same reason [`RuntimeConfig::refresh_interval_seconds`](fabric_tenant_runtime::RuntimeConfig)
    /// does: zero would spin the retry loop in a tight loop renegotiating a
    /// connector that only just failed.
    fn validate_timeouts(&self) -> Result<(), String> {
        if self.request_timeout_seconds == 0 {
            return Err("request_timeout_seconds must be greater than zero".to_owned());
        }

        if self.connector_retry_interval_seconds == 0 {
            return Err("connector_retry_interval_seconds must be greater than zero".to_owned());
        }

        if let Some(longest) = self
            .connectors
            .iter()
            .map(|connector| connector.http_timeout_seconds)
            .max()
        {
            if self.request_timeout_seconds < longest {
                return Err(format!(
                    "request_timeout_seconds ({}) must be at least as long as the longest configured \
                     connector timeout ({longest}s); otherwise the overall budget always expires before \
                     that connector's own timeout could",
                    self.request_timeout_seconds
                ));
            }
        }

        Ok(())
    }

    /// Requires tenant bindings and DataSources to come from different files.
    ///
    /// They are reconciled independently and each source is loaded as a
    /// complete set, so pointing both at one file would mean each load parsed
    /// the other's records and removed everything it did not recognise.
    fn validate_state_paths(&self) -> Result<(), String> {
        if self.tenants_path == self.data_sources_path {
            return Err(
                "tenants_path and data_sources_path must differ: the two resources are \
                        reconciled independently and cannot share a file"
                    .to_owned(),
            );
        }

        Ok(())
    }
}
