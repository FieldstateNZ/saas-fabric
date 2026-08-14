//! What a write mapping must say about where its arguments go.
//!
//! Three checks that need only the configuration to run, so they run before any
//! connector is contacted. The complementary check — that the argument *names*
//! are ones the connector's procedures actually declare — needs the schema, and
//! lives in `registration::procedure_arguments`.

use crate::config::{NdcConnectorConfig, ProcedureBinding};

impl NdcConnectorConfig {
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
    pub(super) fn validate_predicate_arguments(&self) -> Result<(), String> {
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

    /// Requires every insert and update mapping to declare where the payload
    /// goes.
    ///
    /// Unlike its predicate sibling this one is not a data-safety check: a
    /// mapping without a `payload_argument` fails closed, refusing every write
    /// with `InvalidOperation` at translation time. Nothing is lost and nothing
    /// leaks.
    ///
    /// It is here because failing closed on *every* request is not a good place
    /// to discover a typo. The mapping is startup-detectable and wrong in a way
    /// no execution could fix, which is precisely the class of problem the
    /// sibling check exists to catch at boot rather than defer to production
    /// traffic.
    ///
    /// Deletes are absent by design: a delete carries no payload.
    pub(super) fn validate_payload_arguments(&self) -> Result<(), String> {
        for (collection, procedures) in &self.procedures {
            for (operation, binding) in procedures.payload_bearing() {
                let Some(binding) = binding else { continue };

                if binding.payload_argument.is_none() {
                    return Err(format!(
                        "connector {}: {collection}.{operation} needs a payload_argument, otherwise \
                         every {operation} is refused at execution time with nowhere to put the row \
                         values",
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
    pub(super) fn validate_distinct_arguments(&self) -> Result<(), String> {
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
