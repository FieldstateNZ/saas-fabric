//! Checks that span more than one domain.

use std::collections::BTreeSet;

use crate::config::AppConfig;

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
        self.validate_state_paths()
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
