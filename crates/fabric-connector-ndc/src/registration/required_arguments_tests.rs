//! Refusing a mapping that leaves a procedure's required argument unfilled.

use std::collections::BTreeMap;

use crate::config::{CollectionProcedures, PayloadShape, ProcedureBinding};
use crate::registration::required_arguments::check_required_arguments;
use crate::wire::NdcSchemaResponse;
use crate::{NdcConnectorConfig, SchemaIndex};

/// `delete_articles_by_id_and_tenant_key`, shaped exactly as the real
/// `ndc-postgres` v3.1.0 schema declares it (issue #62's capture): two
/// required `text` key arguments and a nullable predicate.
const KEYED_DELETE_SCHEMA: &str = r#"{
    "procedures": [
        {"name": "delete_articles_by_id_and_tenant_key", "arguments": {
            "key_id": {"type": {"type": "named", "name": "text"}},
            "key_tenant_key": {"type": {"type": "named", "name": "text"}},
            "pre_check": {"type": {"type": "nullable", "underlying_type":
                {"type": "predicate", "object_type_name": "articles"}}}
        }}
    ]
}"#;

fn keyed_index() -> SchemaIndex {
    SchemaIndex::build(&serde_json::from_str::<NdcSchemaResponse>(KEYED_DELETE_SCHEMA).unwrap())
}

fn keyed_delete(key_arguments: BTreeMap<String, String>) -> NdcConnectorConfig {
    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: None,
            filter_argument: Some("pre_check".to_owned()),
            key_arguments,
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    NdcConnectorConfig::for_test(BTreeMap::from([("articles".to_owned(), procedures)]))
}

#[test]
fn a_mapping_supplying_both_required_keys_is_accepted() {
    let config = keyed_delete(BTreeMap::from([
        ("id".to_owned(), "key_id".to_owned()),
        ("tenant_key".to_owned(), "key_tenant_key".to_owned()),
    ]));

    assert!(check_required_arguments(&config, &keyed_index()).is_ok());
}

/// F3, restated as a startup failure instead of a first-write one: before
/// `key_arguments` existed, this exact mapping (`filter_argument` only)
/// passed every check this crate ran and failed on the connector's first
/// delete.
#[test]
fn a_mapping_missing_a_required_key_is_refused_at_startup() {
    let config = keyed_delete(BTreeMap::from([("id".to_owned(), "key_id".to_owned())]));

    let error = check_required_arguments(&config, &keyed_index()).unwrap_err();

    assert!(error.contains("articles.delete"), "{error}");
    assert!(error.contains("delete_articles_by_id_and_tenant_key"), "{error}");
    assert!(error.contains("requires `key_tenant_key`"), "{error}");
    assert!(error.contains("supplies nothing for it"), "{error}");
}

#[test]
fn a_mapping_supplying_no_keys_at_all_is_refused() {
    let config = keyed_delete(BTreeMap::new());

    let error = check_required_arguments(&config, &keyed_index()).unwrap_err();

    // Whichever required argument the map visits first is named; both are
    // missing, so only the "refused" outcome is asserted here.
    assert!(error.contains("requires `key_"), "{error}");
}

#[test]
fn a_nullable_argument_is_never_required() {
    // `pre_check` is nullable and unmapped here; only the two non-nullable
    // keys must be covered.
    let config = keyed_delete(BTreeMap::from([
        ("id".to_owned(), "key_id".to_owned()),
        ("tenant_key".to_owned(), "key_tenant_key".to_owned()),
    ]));

    assert!(check_required_arguments(&config, &keyed_index()).is_ok());
}

/// The pre-existing `delete_customers(filter)` shape has one argument, and it
/// is nullable in none of this crate's other fixtures — this pins that a
/// procedure with no non-nullable arguments at all imposes nothing extra.
#[test]
fn a_procedure_with_no_required_arguments_has_nothing_to_check() {
    let schema: NdcSchemaResponse = serde_json::from_str(
        r#"{"procedures": [{"name": "delete_customers", "arguments": {
            "filter": {"type": {"type": "nullable", "underlying_type":
                {"type": "predicate", "object_type_name": "customers"}}}
        }}]}"#,
    )
    .unwrap();

    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_customers".to_owned(),
            payload_argument: None,
            filter_argument: Some("filter".to_owned()),
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };
    let config = NdcConnectorConfig::for_test(BTreeMap::from([("customers".to_owned(), procedures)]));

    assert!(check_required_arguments(&config, &SchemaIndex::build(&schema)).is_ok());
}

#[test]
fn a_read_only_connector_has_nothing_to_check() {
    let config = NdcConnectorConfig::for_test(BTreeMap::new());

    assert!(check_required_arguments(&config, &keyed_index()).is_ok());
}
