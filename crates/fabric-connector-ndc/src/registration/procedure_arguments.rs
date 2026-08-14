//! Checking a write mapping against the procedures the connector declares.

use crate::config::ProcedureBinding;
use crate::schema_index::ArgumentKind;
use crate::{NdcConnectorConfig, SchemaIndex};

/// Refuses a write mapping naming a procedure or argument the connector does
/// not declare.
///
/// # What goes wrong without this
///
/// A mapping saying `filter_argument: "where"` against a procedure declaring
/// `filter` is accepted everywhere else: configuration validation sees a name
/// and cannot know it is wrong, and translation dutifully puts the tenant
/// predicate under `where`. The request goes out carrying the predicate under a
/// name the procedure never declared, leaving the argument it *does* declare
/// empty — and a connector that ignores unknown arguments runs the delete with
/// no predicate at all. Every tenant's rows, `200 OK`, and nothing in the
/// response to say so.
///
/// Same failure shape as the routing check next door, caught the same way: read
/// what the connector declared, and hold the configuration to it before serving
/// anything.
///
/// # Why the type is checked too, and only this far
///
/// A `filter_argument` pointing at a non-predicate argument is as broken as one
/// pointing at nothing — the predicate would land somewhere the procedure
/// cannot use as a predicate. `schema_response.jsonschema` types a predicate
/// argument `{"type": "predicate", "object_type_name": …}`, so that check is
/// exact rather than a guess.
///
/// No equivalent claim is made about the payload's shape, which is
/// connector-defined. The only thing asserted of it is that it is not a
/// predicate argument, which no payload ever is.
///
/// # Errors
///
/// A message naming the collection, the verb, the setting, the argument, and
/// what the procedure actually declares.
pub(super) fn check_procedure_arguments(
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

/// The argument names one mapping declares, each with the setting that named it
/// and the kind that setting requires.
///
/// Pairing the name with its required kind keeps the check below from
/// re-deriving which of the two is the predicate one: that fact belongs to the
/// setting, not to the loop reading it.
fn requirements(binding: &ProcedureBinding) -> [(&'static str, Option<&String>, ArgumentKind); 2] {
    [
        (
            "payload_argument",
            binding.payload_argument.as_ref(),
            ArgumentKind::Value,
        ),
        (
            "filter_argument",
            binding.filter_argument.as_ref(),
            ArgumentKind::Predicate,
        ),
    ]
}

/// Checks one mapping's procedure and both of its argument names.
fn check_binding(
    config: &NdcConnectorConfig,
    index: &SchemaIndex,
    collection: &str,
    verb: &str,
    binding: &ProcedureBinding,
) -> Result<(), String> {
    let procedure = &binding.procedure;
    let where_ = format!("connector {}: {collection}.{verb}", config.id);

    if !index.has_procedure(procedure) {
        return Err(format!(
            "{where_} maps to procedure `{procedure}`, which the connector's schema does not declare"
        ));
    }

    for (setting, argument, required) in requirements(binding) {
        let Some(argument) = argument else { continue };

        match index.procedure_argument(procedure, argument) {
            None => {
                return Err(format!(
                    "{where_} names `{argument}` as its {setting}, but procedure `{procedure}` {}; an \
                     argument a procedure never declared may be silently ignored, which for a \
                     filter_argument means the write runs unscoped",
                    declares(index, procedure)
                ))
            }
            Some(kind) if kind != required => {
                return Err(format!(
                    "{where_} names `{argument}` as its {setting}, but procedure `{procedure}` declares \
                     that argument as {}; a {setting} must be {}",
                    kind.describe(),
                    required.describe()
                ))
            }
            Some(_) => {}
        }
    }

    Ok(())
}

/// What a procedure declares, phrased for the end of a sentence.
fn declares(index: &SchemaIndex, procedure: &str) -> String {
    let declared = index.declared_arguments(procedure);

    if declared.is_empty() {
        "declares no arguments at all".to_owned()
    } else {
        format!("declares only: {}", declared.join(", "))
    }
}
