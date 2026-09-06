//! Neutral [`MutationSpec`] to an NDC mutation request.

use std::collections::BTreeMap;

use fabric_connector::{ConnectorError, MutationSpec, UnsupportedFeature};
use serde_json::Value;

use crate::config::ProcedureBinding;
use crate::schema_index::ArgumentKind;
use crate::translate::procedure_arguments as arguments;
use crate::wire::{NdcMutationFields, NdcMutationOperation, NdcMutationRequest};
use crate::{NdcConnectorConfig, SchemaIndex};

/// Builds the `POST /mutation` body for a targeted mutation.
///
/// `spec` must already have been through
/// [`MutationSpec::for_target`](fabric_connector::MutationSpec::for_target),
/// which scopes the predicate and stamps the tenant discriminator onto written
/// rows.
///
/// # Errors
///
/// - [`ConnectorError::Unsupported`] if the collection has no procedure mapped
///   for this operation.
/// - [`ConnectorError::InvalidOperation`] if the mapping is incomplete or names
///   a procedure the connector does not expose.
/// - [`ConnectorError::Unsupported`] if the predicate cannot be expressed.
pub(crate) fn to_mutation_request(
    spec: &MutationSpec,
    request_arguments: Option<BTreeMap<String, Value>>,
    config: &NdcConnectorConfig,
    index: &SchemaIndex,
) -> Result<NdcMutationRequest, ConnectorError> {
    let collection = spec.collection();

    // "writes to this collection" rather than its name: the caller knows which
    // collection it asked about, and the physical one would end up in a
    // response body. The type is what makes that a certainty rather than a
    // habit — see `UnsupportedFeature`.
    let procedures = config.procedures.get(collection.as_str()).ok_or_else(|| {
        UnsupportedFeature::WritesToCollection
            .refused_because(format!("no procedure mapping is configured for {collection}"))
    })?;

    let verb = spec.operation_name();
    let feature = unmapped_verb_feature(spec);

    let (binding, procedure_arguments) = match spec {
        MutationSpec::Insert { rows, .. } => {
            let binding = arguments::require(procedures.insert.as_ref(), feature, verb, collection.as_str())?;
            (binding, arguments::for_insert(binding, rows)?)
        }
        MutationSpec::Update { filter, changes, .. } => {
            let binding = arguments::require(procedures.update.as_ref(), feature, verb, collection.as_str())?;
            let mut built = arguments::payload(binding, arguments::row_to_json(changes))?;
            arguments::add_predicate(&mut built, binding, filter.as_ref(), spec, index)?;
            (binding, built)
        }
        MutationSpec::Delete { filter, .. } => {
            let binding = arguments::require(procedures.delete.as_ref(), feature, verb, collection.as_str())?;
            let mut built = BTreeMap::new();
            arguments::add_predicate(&mut built, binding, filter.as_ref(), spec, index)?;
            (binding, built)
        }
    };

    ensure_procedure_accepts(binding, config, index)?;

    Ok(NdcMutationRequest {
        operations: vec![NdcMutationOperation::Procedure {
            name: binding.procedure.clone(),
            arguments: procedure_arguments,
            // Every procedure request must select something back — a real
            // `ndc-postgres` refuses one that omits `fields` outright (see
            // `NdcMutationOperation::Procedure`'s rustdoc). `MutationSpec` has
            // no way for a caller to ask for the written rows, so this is
            // always the minimal accepted selection rather than a choice made
            // per call — see `NdcMutationFields::affected_rows_only`.
            fields: Some(NdcMutationFields::affected_rows_only()),
        }],
        collection_relationships: BTreeMap::new(),
        request_arguments,
    })
}

/// What a caller is told when this verb has no procedure mapped.
///
/// Per verb rather than a blanket "writes", because the distinction is
/// actionable: a resource may accept inserts and refuse deletes, and a caller
/// told only "writes" cannot tell which of its requests to change.
const fn unmapped_verb_feature(spec: &MutationSpec) -> UnsupportedFeature {
    match spec {
        MutationSpec::Insert { .. } => UnsupportedFeature::InsertsOnCollection,
        MutationSpec::Update { .. } => UnsupportedFeature::UpdatesOnCollection,
        MutationSpec::Delete { .. } => UnsupportedFeature::DeletesOnCollection,
    }
}

/// Refuses a mapping the connector's schema does not actually support.
///
/// A typo, or configuration written against a different connector version.
/// Catching it here turns an opaque backend error into a clear one.
///
/// The predicate argument is re-checked as well as the procedure name, and the
/// asymmetry is deliberate: `check_procedure_arguments` already refused this at
/// startup, so reaching here means something built a connector without
/// negotiating it. That is the case the check is for. A wrong payload argument
/// yields a failed write; a wrong `filter_argument` yields a *successful* write
/// against every tenant's rows, which is the one outcome worth paying for
/// twice.
fn ensure_procedure_accepts(
    binding: &ProcedureBinding,
    config: &NdcConnectorConfig,
    index: &SchemaIndex,
) -> Result<(), ConnectorError> {
    if !index.has_procedure(&binding.procedure) {
        return Err(ConnectorError::InvalidOperation(format!(
            "connector {} does not expose a procedure named {}",
            config.id, binding.procedure
        )));
    }

    let Some(filter) = binding.filter_argument.as_ref() else {
        return Ok(());
    };

    if index.procedure_argument(&binding.procedure, filter) == Some(ArgumentKind::Predicate) {
        return Ok(());
    }

    Err(ConnectorError::InvalidOperation(format!(
        "connector {}: procedure {} does not declare {filter} as a predicate argument, so the tenant \
         predicate could not be sent where it would take effect",
        config.id, binding.procedure
    )))
}
