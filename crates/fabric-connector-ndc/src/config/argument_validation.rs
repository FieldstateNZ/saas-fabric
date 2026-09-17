//! What a write mapping must say about where its arguments go.
//!
//! Four checks that need only the configuration to run, so they run before any
//! connector is contacted. The complementary check — that the argument *names*
//! are ones the connector's procedures actually declare — needs the schema, and
//! lives in `registration::procedure_arguments`.
//!
//! This file is in the 121-150 line band the file-size policy asks a reason
//! for: four short checks over where a write mapping's payload and predicate
//! arguments go, sharing one concept — argument placement — and each too
//! small on its own to justify a file of its own without fragmenting that one
//! validation pass across more files than it clarifies.

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

    /// Refuses a delete mapping that declares a `payload_argument`.
    ///
    /// A delete carries no payload at all — there is nothing for a
    /// `payload_argument` to name — so one present on a delete mapping is not
    /// unused configuration to shrug at, it is a mapping written for the
    /// wrong setting — the same mistake that produces
    /// `registration::required_arguments`'s false negative: a mapping meant
    /// to write a keyed procedure's `key_id` value into `key_arguments` and
    /// wrote it into `payload_argument` instead, where translation never
    /// sends it for a delete (see `registration::required_arguments_tests`).
    pub(super) fn validate_delete_has_no_payload_argument(&self) -> Result<(), String> {
        for (collection, procedures) in &self.procedures {
            let Some(binding) = procedures.delete.as_ref() else {
                continue;
            };

            if binding.payload_argument.is_some() {
                return Err(format!(
                    "connector {}: {collection}.delete declares payload_argument, but a delete carries no \
                     payload; remove payload_argument from this mapping",
                    self.id
                ));
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
