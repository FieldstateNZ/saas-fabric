//! Refusing a mapping that leaves one of a procedure's required arguments
//! unfilled.

use crate::config::ProcedureBinding;
use crate::{NdcConnectorConfig, SchemaIndex};

/// Refuses a mapping that does not supply every argument its procedure
/// declares as required.
///
/// # What goes wrong without this
///
/// This is the check that would have turned issue #62's F3 into a boot
/// failure instead of a first-write failure. `ndc-postgres` generates
/// `delete_articles_by_id_and_tenant_key(key_id: text, key_tenant_key: text,
/// pre_check: nullable<predicate>)` — `key_id` and `key_tenant_key` are plain,
/// non-nullable arguments the connector will refuse to run without, and
/// nothing about `payload_argument` or `filter_argument` alone can supply
/// them. Before `key_arguments` existed, a mapping naming only
/// `filter_argument` passed every check this crate ran and failed on the
/// connector's first delete.
///
/// "Supplied" means named *anywhere* in the mapping — as
/// [`ProcedureBinding::payload_argument`], [`ProcedureBinding::filter_argument`],
/// or a value in [`ProcedureBinding::key_arguments`]. Which of the three is
/// beside the point; what matters is that the connector's required argument
/// has somewhere to come from.
///
/// # Errors
///
/// A message naming the collection, the verb, the procedure, and the specific
/// required argument the mapping never fills.
pub(super) fn check_required_arguments(
    config: &NdcConnectorConfig,
    index: &SchemaIndex,
) -> Result<(), String> {
    for (collection, procedures) in &config.procedures {
        for (verb, binding) in procedures.all() {
            let Some(binding) = binding else { continue };

            check_binding(config, index, collection, verb, binding)?;
        }
    }

    Ok(())
}

/// Checks one mapping's coverage of its procedure's required arguments.
fn check_binding(
    config: &NdcConnectorConfig,
    index: &SchemaIndex,
    collection: &str,
    verb: &str,
    binding: &ProcedureBinding,
) -> Result<(), String> {
    let procedure = &binding.procedure;
    let supplied = supplied_arguments(binding);

    for required in index.required_arguments(procedure) {
        if supplied.iter().any(|argument| argument.as_str() == required) {
            continue;
        }

        return Err(format!(
            "connector {}: {collection}.{verb} maps to procedure `{procedure}`, which requires `{required}`, \
             and the mapping supplies nothing for it; a required argument this platform never sends means \
             the connector refuses every {verb}",
            config.id
        ));
    }

    Ok(())
}

/// Every argument name one mapping actually fills, regardless of which
/// setting names it.
fn supplied_arguments(binding: &ProcedureBinding) -> Vec<&String> {
    binding
        .payload_argument
        .iter()
        .chain(binding.filter_argument.iter())
        .chain(binding.key_arguments.values())
        .collect()
}
