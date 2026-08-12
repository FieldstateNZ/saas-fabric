//! Checks a connector configuration before anything is built.

use crate::config::NdcConnectorConfig;

impl NdcConnectorConfig {
    /// Checks the configuration is usable and safe.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending setting.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_transport()?;
        self.validate_predicate_arguments()
    }

    /// Rejects a connector that could never be reached, or whose two HTTP
    /// timeouts contradict each other.
    fn validate_transport(&self) -> Result<(), String> {
        if self.endpoint.trim().is_empty() {
            return Err(format!("connector {}: endpoint must not be empty", self.id));
        }

        if self.http_timeout_seconds == 0 {
            return Err(format!(
                "connector {}: http_timeout_seconds must be greater than zero",
                self.id
            ));
        }

        if self.http_connect_timeout_seconds == 0 {
            return Err(format!(
                "connector {}: http_connect_timeout_seconds must be greater than zero",
                self.id
            ));
        }

        // The connect timeout is a subset of the total timeout, not a second
        // budget alongside it. One that outlasts the total would never bind —
        // the total timeout would always fire first — so it is rejected as
        // configuration that cannot mean what it says.
        if self.http_connect_timeout_seconds > self.http_timeout_seconds {
            return Err(format!(
                "connector {}: http_connect_timeout_seconds must not exceed http_timeout_seconds",
                self.id
            ));
        }

        Ok(())
    }

    /// Requires every update and delete mapping to declare where the predicate
    /// goes.
    ///
    /// **This is the check that matters in this file.** Core NDC mutations are
    /// procedure calls, so the predicate that scopes a write to one tenant has
    /// to be passed as a named argument. If the mapping does not say which
    /// argument that is, the predicate has nowhere to go — and a delete that
    /// `MutationSpec::for_target` carefully scoped to one tenant would reach
    /// every tenant's rows on that DataSource.
    ///
    /// Caught here, at startup, rather than at the first delete. The
    /// translation layer refuses it again at execution time; both checks are
    /// deliberate, because the cost of this one failing open is losing other
    /// tenants' data.
    fn validate_predicate_arguments(&self) -> Result<(), String> {
        for (collection, procedures) in &self.procedures {
            for (operation, binding) in procedures.predicate_bearing() {
                let Some(binding) = binding else { continue };

                if binding.filter_argument.is_none() {
                    return Err(format!(
                        "connector {}: {collection}.{operation} needs a filter_argument, otherwise the \
                         tenant predicate would be dropped and the write would reach every tenant's rows",
                        self.id
                    ));
                }
            }
        }

        Ok(())
    }
}
