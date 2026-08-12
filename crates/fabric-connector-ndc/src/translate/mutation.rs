//! Neutral [`MutationSpec`] to an NDC mutation request.

use std::collections::BTreeMap;

use fabric_connector::{ConnectorError, Filter, MutationSpec, Row};
use serde_json::Value;

use crate::config::ProcedureBinding;
use crate::translate::to_expression;
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

    let (binding, arguments) = match spec {
        MutationSpec::Insert { rows, .. } => {
            let binding = require(procedures.insert.as_ref(), "insert", collection.as_str())?;
            (binding, insert_arguments(binding, rows)?)
        }
        MutationSpec::Update { filter, changes, .. } => {
            let binding = require(procedures.update.as_ref(), "update", collection.as_str())?;
            let mut arguments = payload(binding, row_to_json(changes))?;
            add_predicate(&mut arguments, binding, filter.as_ref(), spec, index)?;
            (binding, arguments)
        }
        MutationSpec::Delete { filter, .. } => {
            let binding = require(procedures.delete.as_ref(), "delete", collection.as_str())?;
            let mut arguments = BTreeMap::new();
            add_predicate(&mut arguments, binding, filter.as_ref(), spec, index)?;
            (binding, arguments)
        }
    };

    // A mapping can name a procedure the connector does not actually expose —
    // a typo, or configuration written against a different connector version.
    // Catching it here turns an opaque backend error into a clear one.
    if !index.has_procedure(&binding.procedure) {
        return Err(ConnectorError::InvalidOperation(format!(
            "connector {} does not expose a procedure named {}",
            config.id, binding.procedure
        )));
    }

    Ok(NdcMutationRequest {
        operations: vec![NdcMutationOperation::Procedure {
            name: binding.procedure.clone(),
            arguments,
            fields: None,
        }],
        collection_relationships: BTreeMap::new(),
        request_arguments,
    })
}

/// Requires a procedure mapping for an operation.
fn require<'a>(
    binding: Option<&'a ProcedureBinding>,
    operation: &str,
    collection: &str,
) -> Result<&'a ProcedureBinding, ConnectorError> {
    binding.ok_or_else(|| ConnectorError::Unsupported {
        feature: format!("{operation} on {collection}"),
    })
}

/// Builds the arguments for an insert.
fn insert_arguments(
    binding: &ProcedureBinding,
    rows: &[Row],
) -> Result<BTreeMap<String, Value>, ConnectorError> {
    let objects = Value::Array(rows.iter().map(row_to_json).collect());

    payload(binding, objects)
}

/// Places the payload under its configured argument name.
fn payload(binding: &ProcedureBinding, value: Value) -> Result<BTreeMap<String, Value>, ConnectorError> {
    let name = binding.payload_argument.as_ref().ok_or_else(|| {
        ConnectorError::InvalidOperation(format!(
            "procedure {} needs a payload_argument to carry the rows to write",
            binding.procedure
        ))
    })?;

    Ok(BTreeMap::from([(name.clone(), value)]))
}

/// Places the predicate under its configured argument name.
///
/// The predicate is sent as an NDC expression, which is what a procedure
/// argument of NDC's `predicate` type expects.
///
/// A missing `filter_argument` is refused rather than skipped. Skipping it
/// would drop the tenant scoping that `for_target` just added, turning a
/// tenant-scoped delete into a table-wide one. Configuration validation catches
/// this at startup too; this is the second line of defence.
fn add_predicate(
    arguments: &mut BTreeMap<String, Value>,
    binding: &ProcedureBinding,
    filter: Option<&Filter>,
    spec: &MutationSpec,
    index: &SchemaIndex,
) -> Result<(), ConnectorError> {
    let Some(name) = binding.filter_argument.as_ref() else {
        return Err(ConnectorError::InvalidOperation(format!(
            "procedure {} has no filter_argument, so the tenant predicate could not be sent",
            binding.procedure
        )));
    };

    let Some(filter) = filter else {
        return Err(ConnectorError::InvalidOperation(format!(
            "a {} on {} reached the connector with no predicate",
            spec.operation_name(),
            spec.collection()
        )));
    };

    let expression = to_expression(spec.collection(), filter, index)?;

    let encoded = serde_json::to_value(&expression).map_err(|error| {
        ConnectorError::InvalidOperation(format!("could not encode the predicate: {error}"))
    })?;

    arguments.insert(name.clone(), encoded);

    Ok(())
}

/// Converts a neutral row to a JSON object.
fn row_to_json(row: &Row) -> Value {
    Value::Object(
        row.as_map()
            .iter()
            .map(|(field, value)| (field.to_string(), value.clone()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use fabric_connector::{CollectionName, ComparisonOperator, ConnectorId, FieldName};

    use super::*;
    use crate::config::CollectionProcedures;
    use crate::wire::NdcSchemaResponse;

    fn index() -> SchemaIndex {
        let schema: NdcSchemaResponse = serde_json::from_str(
            r#"{
                "scalar_types": {"text": {"comparison_operators": {"_eq": {"type": "equal"}}}},
                "object_types": {"customers": {"fields": {
                    "name": {"type": {"type": "named", "name": "text"}},
                    "tenant_key": {"type": {"type": "named", "name": "text"}}
                }}},
                "collections": [{"name": "customers", "type": "customers"}],
                "procedures": [
                    {"name": "insert_customers"},
                    {"name": "delete_customers"}
                ]
            }"#,
        )
        .unwrap();

        SchemaIndex::build(&schema)
    }

    fn config_with(procedures: CollectionProcedures) -> NdcConnectorConfig {
        NdcConnectorConfig {
            id: ConnectorId::try_new("postgres").unwrap(),
            endpoint: "http://connector".to_owned(),
            timeout_seconds: 10,
            connection_name_argument: "connection_name".to_owned(),
            connection_string_argument: "connection_string".to_owned(),
            procedures: BTreeMap::from([("customers".to_owned(), procedures)]),
        }
    }

    fn collection() -> CollectionName {
        CollectionName::try_new("customers").unwrap()
    }

    fn tenant_predicate() -> Filter {
        Filter::Compare {
            field: FieldName::try_new("tenant_key").unwrap(),
            operator: ComparisonOperator::Equal,
            value: Value::String("tenant-482".to_owned()),
        }
    }

    #[test]
    fn an_insert_becomes_a_procedure_call_carrying_the_rows() {
        let config = config_with(CollectionProcedures {
            insert: Some(ProcedureBinding {
                procedure: "insert_customers".to_owned(),
                payload_argument: Some("objects".to_owned()),
                filter_argument: None,
            }),
            ..CollectionProcedures::default()
        });

        let spec = MutationSpec::Insert {
            collection: collection(),
            rows: vec![Row::new().with(FieldName::try_new("name").unwrap(), Value::String("Alice".into()))],
        };

        let request = to_mutation_request(&spec, None, &config, &index()).unwrap();
        let NdcMutationOperation::Procedure { name, arguments, .. } = request.operations.first().unwrap();

        assert_eq!(name, "insert_customers");
        assert_eq!(arguments["objects"][0]["name"], "Alice");
    }

    #[test]
    fn a_collection_with_no_mapping_cannot_be_written_to() {
        let config = config_with(CollectionProcedures::default());
        let spec = MutationSpec::Insert {
            collection: collection(),
            rows: vec![Row::new()],
        };

        assert!(matches!(
            to_mutation_request(&spec, None, &config, &index()).unwrap_err(),
            ConnectorError::Unsupported { .. }
        ));
    }

    #[test]
    fn a_delete_sends_its_predicate_as_an_ndc_expression() {
        let config = config_with(CollectionProcedures {
            delete: Some(ProcedureBinding {
                procedure: "delete_customers".to_owned(),
                payload_argument: None,
                filter_argument: Some("filter".to_owned()),
            }),
            ..CollectionProcedures::default()
        });

        let spec = MutationSpec::Delete {
            collection: collection(),
            filter: Some(tenant_predicate()),
        };

        let request = to_mutation_request(&spec, None, &config, &index()).unwrap();
        let NdcMutationOperation::Procedure { arguments, .. } = request.operations.first().unwrap();

        assert_eq!(arguments["filter"]["type"], "binary_comparison_operator");
        assert_eq!(arguments["filter"]["value"]["value"], "tenant-482");
    }

    #[test]
    fn a_delete_that_arrives_without_a_predicate_is_refused() {
        // `for_target` always adds one under discriminator isolation. Reaching
        // here without one means something bypassed it, and executing an
        // unscoped delete would be catastrophic.
        let config = config_with(CollectionProcedures {
            delete: Some(ProcedureBinding {
                procedure: "delete_customers".to_owned(),
                payload_argument: None,
                filter_argument: Some("filter".to_owned()),
            }),
            ..CollectionProcedures::default()
        });

        let spec = MutationSpec::Delete {
            collection: collection(),
            filter: None,
        };

        assert!(matches!(
            to_mutation_request(&spec, None, &config, &index()).unwrap_err(),
            ConnectorError::InvalidOperation(_)
        ));
    }

    #[test]
    fn a_mapping_naming_a_procedure_the_connector_lacks_is_refused() {
        let config = config_with(CollectionProcedures {
            insert: Some(ProcedureBinding {
                procedure: "insert_custmers".to_owned(),
                payload_argument: Some("objects".to_owned()),
                filter_argument: None,
            }),
            ..CollectionProcedures::default()
        });

        let spec = MutationSpec::Insert {
            collection: collection(),
            rows: vec![Row::new()],
        };

        let error = to_mutation_request(&spec, None, &config, &index()).unwrap_err();
        assert!(matches!(error, ConnectorError::InvalidOperation(_)));
    }
}
