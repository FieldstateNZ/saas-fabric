//! Checks a connector configuration before anything is built.
//!
//! The entry point and the transport checks. The write-mapping checks are next
//! door in [`argument_validation`](super::argument_validation), because they
//! are a different subject and considerably more interesting.

use crate::config::NdcConnectorConfig;

impl NdcConnectorConfig {
    /// Checks the configuration is usable and safe.
    ///
    /// Everything here is answerable from the configuration alone, which is why
    /// it runs before the connector is contacted. Checks that need the
    /// connector's own `/schema` — that a mapped procedure exists, and that
    /// every argument named here is one it declares — run at negotiation, in
    /// `registration`.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending setting.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_transport()?;
        self.validate_predicate_arguments()?;
        self.validate_payload_arguments()?;
        self.validate_distinct_arguments()
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
}
