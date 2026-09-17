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
/// "Supplied" means named by whichever settings *this verb's translation
/// actually sends* — see [`supplied_arguments`], which is verb-aware for
/// exactly this reason. An argument named by a setting translation never
/// reads for this verb (a `payload_argument` on a delete mapping, say) is not
/// supplied, whatever the configuration happens to say: the connector would
/// still refuse the call, because nothing on the wire ever named it.
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
    let supplied = supplied_arguments(binding, verb);

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

/// Every argument name one mapping actually sends for `verb`, matching what
/// `translate::mutation::to_mutation_request` builds for each operation: an
/// insert sends only its payload; an update sends its payload, its predicate,
/// and every key argument; a delete sends its predicate and every key
/// argument, and never a payload — a delete has none, and config validation
/// ([`crate::config::NdcConnectorConfig::validate_delete_has_no_payload_argument`])
/// refuses a mapping that declares one anyway.
///
/// Getting this wrong in either direction is a real bug, not a style choice.
/// Undercounting refuses a mapping that would have worked. Overcounting —
/// crediting a delete with an argument only `payload_argument` names — is a
/// false negative a verb-agnostic version of this function produced: a
/// `delete_articles_by_id_and_tenant_key` mapping that wrote its `key_id`
/// value into `payload_argument` instead of `key_arguments` passed this check
/// by coincidence, because that version counted `payload_argument` regardless
/// of verb. See `required_arguments_tests` for the mapping that pins this
/// against the real procedure shape.
fn supplied_arguments<'a>(binding: &'a ProcedureBinding, verb: &str) -> Vec<&'a String> {
    let payload = binding.payload_argument.iter();
    let filter = binding.filter_argument.iter();
    let keys = binding.key_arguments.values();

    match verb {
        "insert" => payload.collect(),
        "update" => payload.chain(filter).chain(keys).collect(),
        "delete" => filter.chain(keys).collect(),
        // `CollectionProcedures::all` names only these three verbs today, so
        // this arm is unreached. It fails closed rather than being left
        // absent: "supplies nothing" refuses a fourth verb's mapping at
        // startup whenever its procedure declares a required argument, which
        // is loud and safe, instead of a panic on an unmatched pattern or a
        // silent "supplies everything" that would refuse nothing this check
        // exists to catch.
        _ => Vec::new(),
    }
}
