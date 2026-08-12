//! Mapping neutral writes onto procedure calls.

use std::collections::BTreeMap;

use fabric_connector::{
    CollectionName, ComparisonOperator, ConnectorError, ConnectorId, FieldName, Filter, MutationSpec, Row,
};
use serde_json::Value;

use crate::config::{CollectionProcedures, ProcedureBinding};
use crate::translate::to_mutation_request;
use crate::wire::{NdcMutationOperation, NdcSchemaResponse};
use crate::{NdcConnectorConfig, SchemaIndex};

fn index() -> SchemaIndex {
    let schema: NdcSchemaResponse = serde_json::from_str(
        r#"{
            "scalar_types": {"text": {"comparison_operators": {"_eq": {"type": "equal"}}}},
            "object_types": {"customers": {"fields": {
                "name": {"type": {"type": "named", "name": "text"}},
                "tenant_key": {"type": {"type": "named", "name": "text"}}
            }}},
            "collections": [{"name": "customers", "type": "customers"}],
            "procedures": [{"name": "insert_customers"}, {"name": "delete_customers"}]
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

fn insert_binding() -> ProcedureBinding {
    ProcedureBinding {
        procedure: "insert_customers".to_owned(),
        payload_argument: Some("objects".to_owned()),
        filter_argument: None,
    }
}

fn delete_binding() -> ProcedureBinding {
    ProcedureBinding {
        procedure: "delete_customers".to_owned(),
        payload_argument: None,
        filter_argument: Some("filter".to_owned()),
    }
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
        insert: Some(insert_binding()),
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
        delete: Some(delete_binding()),
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
    // `for_target` always adds one under discriminator isolation. Reaching here
    // without one means something bypassed it, and an unscoped delete would be
    // catastrophic.
    let config = config_with(CollectionProcedures {
        delete: Some(delete_binding()),
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
            ..insert_binding()
        }),
        ..CollectionProcedures::default()
    });

    let spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![Row::new()],
    };

    assert!(matches!(
        to_mutation_request(&spec, None, &config, &index()).unwrap_err(),
        ConnectorError::InvalidOperation(_)
    ));
}
