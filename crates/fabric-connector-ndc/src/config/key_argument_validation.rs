//! What a write mapping must say about its key arguments and payload shape.
//!
//! Three checks that, like their siblings in `argument_validation`, need only
//! the configuration to run. The complementary checks — that a key argument
//! is one the procedure actually declares, that it is a value and not a
//! predicate, and that the procedure's own required arguments are all
//! supplied — need the schema, and live in `registration::key_arguments` and
//! `registration::required_arguments`.

use crate::config::{NdcConnectorConfig, PayloadShape, ProcedureBinding};

impl NdcConnectorConfig {
    /// Refuses an insert mapping that declares `key_arguments`.
    ///
    /// An insert has no key to scope by — it creates rows, it does not select
    /// among existing ones — so a key mapping on one can only be leftover
    /// configuration from an update or delete, silently ignored by
    /// translation. Refusing it at startup turns that silence into a message
    /// naming the mistake instead.
    pub(super) fn validate_key_arguments_on_insert(&self) -> Result<(), String> {
        for (collection, procedures) in &self.procedures {
            let Some(binding) = procedures.insert.as_ref() else {
                continue;
            };

            if !binding.key_arguments.is_empty() {
                return Err(format!(
                    "connector {}: {collection}.insert declares key_arguments, but an insert has no key to \
                     scope by; remove key_arguments from this mapping",
                    self.id
                ));
            }
        }

        Ok(())
    }

    /// Requires every key argument name to be distinct from every other
    /// argument name the same mapping uses.
    ///
    /// Payload, predicate, and every key value are all written into the same
    /// argument map, so any two settings sharing a name mean the second write
    /// silently overwrites the first — the same reasoning as
    /// [`Self::validate_distinct_arguments`], extended to cover the new
    /// argument names this mapping can name.
    pub(super) fn validate_key_argument_distinctness(&self) -> Result<(), String> {
        for (collection, procedures) in &self.procedures {
            for (operation, binding) in procedures.all() {
                let Some(binding) = binding else { continue };

                check_binding_distinctness(&self.id, collection, operation, binding)?;
            }
        }

        Ok(())
    }

    /// Requires `payload_shape: set_operations` only on an update mapping.
    ///
    /// An insert sends an array of row objects — there is no per-column
    /// argument to wrap in `{"_set": ...}` — and a delete sends no payload at
    /// all. Nothing has been observed wanting either shaped, so a mapping
    /// asking for it is configuration written for the wrong verb rather than
    /// a case this crate should try to honour.
    ///
    /// Walks [`crate::config::CollectionProcedures::non_update`] rather than listing
    /// `"insert"` and `"delete"` by hand: a verb this crate learns to map in
    /// the future inherits the same constraint automatically, instead of
    /// silently passing this check by omission the way the false negative in
    /// `registration::required_arguments` did for a differently hand-listed
    /// verb set.
    pub(super) fn validate_payload_shape(&self) -> Result<(), String> {
        for (collection, procedures) in &self.procedures {
            for (operation, binding) in procedures.non_update() {
                let Some(binding) = binding else { continue };

                if binding.payload_shape != PayloadShape::Values {
                    return Err(format!(
                        "connector {}: {collection}.{operation} declares payload_shape = set_operations, but \
                         only an update mapping may shape its payload that way; {operation} has no \
                         per-column argument to wrap it in",
                        self.id
                    ));
                }
            }
        }

        Ok(())
    }
}

/// Checks one mapping's key arguments against its own payload and predicate
/// arguments, and against each other.
fn check_binding_distinctness(
    connector: &fabric_connector::ConnectorId,
    collection: &str,
    operation: &str,
    binding: &ProcedureBinding,
) -> Result<(), String> {
    let mut seen: Vec<&str> = Vec::new();
    seen.extend(binding.payload_argument.as_deref());
    seen.extend(binding.filter_argument.as_deref());

    for argument in binding.key_arguments.values() {
        if seen.contains(&argument.as_str()) {
            return Err(format!(
                "connector {connector}: {collection}.{operation} names `{argument}` as a key argument, but it \
                 also names `{argument}` as its payload_argument or filter_argument (or as another key \
                 argument); one argument map, so the second write would overwrite the first"
            ));
        }

        seen.push(argument.as_str());
    }

    Ok(())
}
