//! Neutral [`MutationSpec`] to an NDC mutation request.

use std::collections::BTreeMap;

use fabric_connector::{ConnectorError, MutationSpec};
use serde_json::Value;

use crate::config::ProcedureBinding;
use crate::translate::procedure_arguments as arguments;
use crate::wire::{NdcMutationOperation, NdcMutationRequest};
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

    let procedures =
        config
            .procedures
            .get(collection.as_str())
            .ok_or_else(|| ConnectorError::Unsupported {
                feature: format!("writes to {collection} (no procedure mapping is configured)"),
            })?;

    let (binding, procedure_arguments) = match spec {
        MutationSpec::Insert { rows, .. } => {
            let binding = arguments::require(procedures.insert.as_ref(), "insert", collection.as_str())?;
            (binding, arguments::for_insert(binding, rows)?)
        }
        MutationSpec::Update { filter, changes, .. } => {
            let binding = arguments::require(procedures.update.as_ref(), "update", collection.as_str())?;
            let mut built = arguments::payload(binding, arguments::row_to_json(changes))?;
            arguments::add_predicate(&mut built, binding, filter.as_ref(), spec, index)?;
            (binding, built)
        }
        MutationSpec::Delete { filter, .. } => {
            let binding = arguments::require(procedures.delete.as_ref(), "delete", collection.as_str())?;
            let mut built = BTreeMap::new();
            arguments::add_predicate(&mut built, binding, filter.as_ref(), spec, index)?;
            (binding, built)
        }
    };

    ensure_procedure_exists(binding, config, index)?;

    Ok(NdcMutationRequest {
        operations: vec![NdcMutationOperation::Procedure {
            name: binding.procedure.clone(),
            arguments: procedure_arguments,
            fields: None,
        }],
        collection_relationships: BTreeMap::new(),
        request_arguments,
    })
}

/// Refuses a mapping that names a procedure the connector does not expose.
///
/// A typo, or configuration written against a different connector version.
/// Catching it here turns an opaque backend error into a clear one.
fn ensure_procedure_exists(
    binding: &ProcedureBinding,
    config: &NdcConnectorConfig,
    index: &SchemaIndex,
) -> Result<(), ConnectorError> {
    if index.has_procedure(&binding.procedure) {
        return Ok(());
    }

    Err(ConnectorError::InvalidOperation(format!(
        "connector {} does not expose a procedure named {}",
        config.id, binding.procedure
    )))
}
