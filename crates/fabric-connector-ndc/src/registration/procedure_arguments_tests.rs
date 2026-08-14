//! Holding a write mapping to the arguments the connector actually declares.

use std::collections::BTreeMap;

use crate::config::{CollectionProcedures, ProcedureBinding};
use crate::registration::procedure_arguments::check_procedure_arguments;
use crate::wire::NdcSchemaResponse;
use crate::{NdcConnectorConfig, SchemaIndex};

/// A connector declaring the three procedures with the arguments
/// `ndc-postgres` documents: `objects` for an insert, `update_columns` plus
/// `filter` for an update, `filter` for a delete.
const SCHEMA: &str = r#"{
    "scalar_types": {"text": {"comparison_operators": {"_eq": {"type": "equal"}}}},
    "object_types": {"customers": {"fields": {"name": {"type": {"type": "named", "name": "text"}}}}},
    "collections": [{"name": "customers", "type": "customers"}],
    "procedures": [
        {"name": "insert_customers", "arguments": {
            "objects": {"type": {"type": "array", "element_type": {"type": "named", "name": "customers"}}}
        }},
        {"name": "update_customers", "arguments": {
            "update_columns": {"type": {"type": "named", "name": "customers"}},
            "filter": {"type": {"type": "predicate", "object_type_name": "customers"}}
        }},
        {"name": "delete_customers", "arguments": {
            "filter": {"type": {"type": "predicate", "object_type_name": "customers"}}
        }}
    ]
}"#;

fn index() -> SchemaIndex {
    SchemaIndex::build(&serde_json::from_str::<NdcSchemaResponse>(SCHEMA).unwrap())
}

fn config_with(procedures: CollectionProcedures) -> NdcConnectorConfig {
    NdcConnectorConfig::for_test(BTreeMap::from([("customers".to_owned(), procedures)]))
}

fn delete_with(filter_argument: &str) -> NdcConnectorConfig {
    config_with(CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_customers".to_owned(),
            payload_argument: None,
            filter_argument: Some(filter_argument.to_owned()),
        }),
        ..CollectionProcedures::default()
    })
}

#[test]
fn a_mapping_matching_the_declared_arguments_is_accepted() {
    let config = config_with(CollectionProcedures {
        insert: Some(ProcedureBinding {
            procedure: "insert_customers".to_owned(),
            payload_argument: Some("objects".to_owned()),
            filter_argument: None,
        }),
        update: Some(ProcedureBinding {
            procedure: "update_customers".to_owned(),
            payload_argument: Some("update_columns".to_owned()),
            filter_argument: Some("filter".to_owned()),
        }),
        delete: Some(ProcedureBinding {
            procedure: "delete_customers".to_owned(),
            payload_argument: None,
            filter_argument: Some("filter".to_owned()),
        }),
    });

    assert!(check_procedure_arguments(&config, &index()).is_ok());
}

#[test]
fn a_filter_argument_the_procedure_never_declares_is_refused() {
    // The defect: the procedure declares `filter`, the mapping says `where`,
    // and the tenant predicate used to go out under `where` — leaving a
    // connector that ignores unknown arguments to run an unscoped delete.
    let error = check_procedure_arguments(&delete_with("where"), &index()).unwrap_err();

    assert!(error.contains("`where`"), "{error}");
    assert!(error.contains("filter_argument"), "{error}");
    // Names what the procedure does declare, so an operator can fix it.
    assert!(error.contains("declares only: filter"), "{error}");
}

#[test]
fn a_filter_argument_pointing_at_a_non_predicate_argument_is_refused() {
    // `update_columns` exists, so a name check alone would pass this. It is
    // typed as an object, and a predicate sent there is not a predicate.
    let config = config_with(CollectionProcedures {
        update: Some(ProcedureBinding {
            procedure: "update_customers".to_owned(),
            payload_argument: Some("filter".to_owned()),
            filter_argument: Some("update_columns".to_owned()),
        }),
        ..CollectionProcedures::default()
    });

    let error = check_procedure_arguments(&config, &index()).unwrap_err();

    assert!(error.contains("not a predicate"), "{error}");
}

#[test]
fn a_payload_argument_pointing_at_a_predicate_argument_is_refused() {
    // The mirror of the case above, and the only claim that can honestly be
    // made about a payload argument: whatever shape it has, it is not a
    // predicate.
    let config = config_with(CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_customers".to_owned(),
            payload_argument: Some("filter".to_owned()),
            filter_argument: Some("filter".to_owned()),
        }),
        ..CollectionProcedures::default()
    });

    let error = check_procedure_arguments(&config, &index()).unwrap_err();

    assert!(error.contains("payload_argument"), "{error}");
}

#[test]
fn a_procedure_declaring_no_arguments_rejects_every_configured_name() {
    let schema: NdcSchemaResponse =
        serde_json::from_str(r#"{"procedures": [{"name": "delete_customers"}]}"#).unwrap();

    let error = check_procedure_arguments(&delete_with("filter"), &SchemaIndex::build(&schema)).unwrap_err();

    assert!(error.contains("declares no arguments at all"), "{error}");
}

#[test]
fn a_mapping_naming_a_procedure_the_connector_lacks_is_refused_at_startup() {
    // Translation refuses this too. Both are deliberate — this one means an
    // operator finds out at boot rather than at the first delete.
    let config = config_with(CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_custmers".to_owned(),
            payload_argument: None,
            filter_argument: Some("filter".to_owned()),
        }),
        ..CollectionProcedures::default()
    });

    let error = check_procedure_arguments(&config, &index()).unwrap_err();

    assert!(error.contains("does not declare"), "{error}");
}

#[test]
fn a_read_only_connector_has_nothing_to_check() {
    let config = NdcConnectorConfig::for_test(BTreeMap::new());

    assert!(check_procedure_arguments(&config, &index()).is_ok());
}
