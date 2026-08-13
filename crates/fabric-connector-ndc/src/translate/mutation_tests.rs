//! Mapping neutral writes onto procedure calls.

use std::collections::BTreeMap;

use fabric_connector::{
    CollectionName, ComparisonOperator, ConnectorError, FieldName, Filter, MutationSpec, Row,
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
            "procedures": [
                {"name": "insert_customers"},
                {"name": "update_customers"},
                {"name": "delete_customers"}
            ]
        }"#,
    )
    .unwrap();

    SchemaIndex::build(&schema)
}

fn config_with(procedures: CollectionProcedures) -> NdcConnectorConfig {
    NdcConnectorConfig::for_test(BTreeMap::from([("customers".to_owned(), procedures)]))
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

fn update_binding() -> ProcedureBinding {
    ProcedureBinding {
        procedure: "update_customers".to_owned(),
        payload_argument: Some("update_columns".to_owned()),
        filter_argument: Some("filter".to_owned()),
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

/// An update as `for_target` leaves it under discriminator isolation: the
/// caller's change, the stamped tenant key, and a tenant-scoped predicate.
fn update_spec() -> MutationSpec {
    MutationSpec::Update {
        collection: collection(),
        filter: Some(tenant_predicate()),
        changes: Row::new()
            .with(FieldName::try_new("name").unwrap(), Value::String("Alice".into()))
            .with(
                FieldName::try_new("tenant_key").unwrap(),
                Value::String("tenant-482".into()),
            ),
    }
}

fn update_config(binding: ProcedureBinding) -> NdcConnectorConfig {
    config_with(CollectionProcedures {
        update: Some(binding),
        ..CollectionProcedures::default()
    })
}

#[test]
fn an_update_sends_its_payload_and_its_predicate_under_separate_arguments() {
    // The regression this pins: payload and predicate are written into one
    // argument map, so a mapping naming the same argument twice used to drop
    // the payload entirely and translate without complaint.
    let config = update_config(update_binding());

    let request = to_mutation_request(&update_spec(), None, &config, &index()).unwrap();
    let NdcMutationOperation::Procedure { name, arguments, .. } = request.operations.first().unwrap();

    assert_eq!(name, "update_customers");
    assert_eq!(arguments.len(), 2, "payload and predicate must both survive");
    assert_eq!(arguments["update_columns"]["name"], "Alice");
    assert_eq!(arguments["filter"]["type"], "binary_comparison_operator");
}

#[test]
fn an_update_carries_the_tenant_predicate_through_translation() {
    let config = update_config(update_binding());

    let request = to_mutation_request(&update_spec(), None, &config, &index()).unwrap();
    let NdcMutationOperation::Procedure { arguments, .. } = request.operations.first().unwrap();

    assert_eq!(arguments["filter"]["column"]["name"], "tenant_key");
    assert_eq!(arguments["filter"]["value"]["value"], "tenant-482");
    // The stamped discriminator has to reach the payload too, or an update
    // could move a row out of its tenant.
    assert_eq!(arguments["update_columns"]["tenant_key"], "tenant-482");
}

#[test]
fn an_update_mapping_without_a_filter_argument_is_refused() {
    // Startup validation rejects this as well. Both checks are deliberate:
    // translating it anyway would send an unscoped update.
    let config = update_config(ProcedureBinding {
        filter_argument: None,
        ..update_binding()
    });

    assert!(matches!(
        to_mutation_request(&update_spec(), None, &config, &index()).unwrap_err(),
        ConnectorError::InvalidOperation(_)
    ));
}

#[test]
fn an_update_mapping_without_a_payload_argument_is_refused() {
    let config = update_config(ProcedureBinding {
        payload_argument: None,
        ..update_binding()
    });

    assert!(matches!(
        to_mutation_request(&update_spec(), None, &config, &index()).unwrap_err(),
        ConnectorError::InvalidOperation(_)
    ));
}

#[test]
fn an_update_that_arrives_without_a_predicate_is_refused() {
    // As for a delete: reaching translation with no predicate means something
    // bypassed `for_target`, and a table-wide update would follow.
    let config = update_config(update_binding());

    let spec = MutationSpec::Update {
        collection: collection(),
        filter: None,
        changes: Row::new().with(FieldName::try_new("name").unwrap(), Value::String("Alice".into())),
    };

    assert!(matches!(
        to_mutation_request(&spec, None, &config, &index()).unwrap_err(),
        ConnectorError::InvalidOperation(_)
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
