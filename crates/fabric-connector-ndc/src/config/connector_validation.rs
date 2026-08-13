//! Checks a connector configuration before anything is built.

use crate::config::{NdcConnectorConfig, ProcedureBinding};

impl NdcConnectorConfig {
    /// Checks the configuration is usable and safe.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending setting.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_transport()?;
        self.validate_predicate_arguments()?;
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

    /// Requires a mapping's payload and predicate to land in *different*
    /// arguments.
    ///
    /// One `BTreeMap` carries a procedure's arguments and an update fills it
    /// twice — payload first, predicate second. Name both the same and the
    /// second write wins: the caller's field values are gone, and the write
    /// reports success having changed nothing. Refused at startup rather than
    /// defended against during translation, because no execution of such a
    /// mapping could mean anything, whichever verb it is attached to.
    ///
    /// The connection-routing names are deliberately *not* compared against
    /// these. Routing travels in the request's top-level `request_arguments`, a
    /// different map altogether, so a procedure argument sharing a name with
    /// one cannot displace it — and refusing that pairing would reject a
    /// configuration that works.
    fn validate_distinct_arguments(&self) -> Result<(), String> {
        for (collection, procedures) in &self.procedures {
            for (operation, binding) in procedures.all() {
                // Only a mapping declaring both names can collide.
                let Some((payload, filter)) = binding.and_then(ProcedureBinding::argument_names) else {
                    continue;
                };

                if payload == filter {
                    return Err(format!(
                        "connector {}: {collection}.{operation} names {payload} as both its \
                         payload_argument and its filter_argument, so the predicate would overwrite \
                         the payload and the write would silently change nothing",
                        self.id
                    ));
                }
            }
        }

        Ok(())
    }
}
