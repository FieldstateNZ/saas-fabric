//! Reading a keyed procedure's key values off the predicate the platform
//! already built.

use std::collections::BTreeMap;

use fabric_connector::{ConnectorError, Filter, MutationSpec};
use serde_json::Value;

use crate::config::ProcedureBinding;
use crate::translate::key_equality::single_equality;

/// Adds one argument per [`ProcedureBinding::key_arguments`] entry, sourced
/// from `filter`'s own equality clauses.
///
/// Called after the predicate has already been placed under
/// `filter_argument` (see `procedure_arguments::add_predicate`), on the exact
/// same, post-`for_target` filter — so a key value is read from the predicate
/// the platform built, **never** from the caller's payload and never guessed.
/// When a key field is also the tenant discriminator, its value reaches the
/// connector twice this way: once as a key argument, once inside the
/// predicate. That is defence in depth, not redundancy to trim.
///
/// # A key argument must never displace what is already there
///
/// `arguments` already carries the payload argument (for an update) and the
/// predicate argument by the time this runs. Config validation
/// ([`crate::config::NdcConnectorConfig::validate_key_argument_distinctness`])
/// refuses a mapping whose key argument names collide with either of those,
/// or with each other, before a connector is ever built — but this function
/// checks again rather than trusting that and blindly `insert`ing, for the
/// same reason `translate::mutation::ensure_procedure_accepts` re-checks
/// `filter_argument` even though `registration::procedure_arguments` already
/// refused a wrong one at startup: a config-only guarantee is a promise about
/// configuration that passed validation, not about the value this function is
/// actually holding, and the cost of the two ever disagreeing is one write's
/// key argument silently overwriting its predicate or its payload.
///
/// # Errors
///
/// [`ConnectorError::InvalidOperation`] naming the verb, the collection, the
/// field, and the argument, when: the filter has no equality for a mapped key
/// field; the filter has more than one equality for it with differing values;
/// or the argument name already carries a value this same call wrote earlier
/// — the payload, the predicate, or another key argument. A keyed procedure
/// cannot be called without its key, cannot be called against a contradictory
/// one, and cannot have its key argument overwrite another argument either.
pub(super) fn add_key_arguments(
    arguments: &mut BTreeMap<String, Value>,
    binding: &ProcedureBinding,
    filter: &Filter,
    spec: &MutationSpec,
) -> Result<(), ConnectorError> {
    for (field, argument) in &binding.key_arguments {
        let value = single_equality(filter, field).map_err(|reason| {
            ConnectorError::InvalidOperation(format!(
                "a {} on {} maps key field `{field}` to argument `{argument}`, but {reason}",
                spec.operation_name(),
                spec.collection(),
            ))
        })?;

        if arguments.contains_key(argument) {
            return Err(ConnectorError::InvalidOperation(format!(
                "a {} on {} maps key field `{field}` to argument `{argument}`, but {} already writes \
                 that argument; one argument map, so the key would overwrite it rather than add to it",
                spec.operation_name(),
                spec.collection(),
                existing_setting(binding, argument),
            )));
        }

        arguments.insert(argument.clone(), value.clone());
    }

    Ok(())
}

/// Names whichever setting on `binding` already claims `argument`, for the
/// collision message above.
///
/// Checked in the same order [`add_key_arguments`]'s caller builds
/// `arguments`: payload first, then predicate, then any earlier key argument
/// — so on the rare case a field could be described two ways (which
/// [`crate::config::NdcConnectorConfig::validate_key_argument_distinctness`]
/// does not allow to exist), the message still names something true.
fn existing_setting(binding: &ProcedureBinding, argument: &str) -> String {
    if binding.payload_argument.as_deref() == Some(argument) {
        "payload_argument".to_owned()
    } else if binding.filter_argument.as_deref() == Some(argument) {
        "filter_argument".to_owned()
    } else {
        binding
            .key_arguments
            .iter()
            .find(|(_, other)| other.as_str() == argument)
            .map_or_else(
                || "another setting".to_owned(),
                |(field, _)| format!("the key argument for field `{field}`"),
            )
    }
}
