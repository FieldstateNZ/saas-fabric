//! Holding a mapping's key arguments to what the connector actually declares.

use std::collections::BTreeMap;

use crate::config::{CollectionProcedures, PayloadShape, ProcedureBinding};
use crate::registration::key_arguments::check_key_arguments;
use crate::wire::NdcSchemaResponse;
use crate::{NdcConnectorConfig, SchemaIndex};

/// A connector declaring `delete_articles_by_id_and_tenant_key`, shaped like
/// the real `ndc-postgres` v3.1.0 procedure this crate observed for issue
/// #62/#67: two plain `text` key arguments and a nullable predicate.
const SCHEMA: &str = r#"{
    "scalar_types": {"text": {"comparison_operators": {"_eq": {"type": "equal"}}}},
    "object_types": {"articles": {"fields": {
        "id": {"type": {"type": "named", "name": "text"}},
        "tenant_key": {"type": {"type": "named", "name": "text"}}
    }}},
    "collections": [{"name": "articles", "type": "articles"}],
    "procedures": [
        {"name": "delete_articles_by_id_and_tenant_key", "arguments": {
            "key_id": {"type": {"type": "named", "name": "text"}},
            "key_tenant_key": {"type": {"type": "named", "name": "text"}},
            "pre_check": {"type": {"type": "nullable", "underlying_type":
                {"type": "predicate", "object_type_name": "articles"}}}
        }}
    ]
}"#;

fn index() -> SchemaIndex {
    SchemaIndex::build(&serde_json::from_str::<NdcSchemaResponse>(SCHEMA).unwrap())
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

fn full_keys() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("id".to_owned(), "key_id".to_owned()),
        ("tenant_key".to_owned(), "key_tenant_key".to_owned()),
    ])
}

#[test]
fn a_mapping_naming_the_declared_key_arguments_is_accepted() {
    assert!(check_key_arguments(&keyed_delete(full_keys()), &index()).is_ok());
}

#[test]
fn a_key_argument_the_procedure_never_declares_is_refused() {
    let config = keyed_delete(BTreeMap::from([("id".to_owned(), "key_identifier".to_owned())]));

    let error = check_key_arguments(&config, &index()).unwrap_err();

    assert!(error.contains("`key_identifier`"), "{error}");
    assert!(error.contains("field `id`"), "{error}");
    assert!(error.contains("does not declare it"), "{error}");
}

#[test]
fn a_key_argument_declared_as_a_predicate_is_refused() {
    // `pre_check` exists on the procedure, so a name-only check would pass
    // this. It is predicate-typed, and a key is a scalar value, never a
    // predicate.
    let config = keyed_delete(BTreeMap::from([("id".to_owned(), "pre_check".to_owned())]));

    let error = check_key_arguments(&config, &index()).unwrap_err();

    assert!(error.contains("a key is never a predicate"), "{error}");
}

#[test]
fn a_key_field_absent_from_the_collections_schema_is_refused() {
    let config = keyed_delete(BTreeMap::from([("headline".to_owned(), "key_id".to_owned())]));

    let error = check_key_arguments(&config, &index()).unwrap_err();

    assert!(error.contains("field `headline`"), "{error}");
    assert!(error.contains("has no such field"), "{error}");
}

#[test]
fn a_key_field_on_a_collection_the_schema_never_declares_is_not_this_checks_problem() {
    // "articles" is the only collection in `SCHEMA`; a mapping for a
    // collection the schema does not have at all is a different, pre-existing
    // failure mode, so this check stays quiet about the field rather than
    // doubling up on it.
    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: None,
            filter_argument: Some("pre_check".to_owned()),
            key_arguments: BTreeMap::from([("id".to_owned(), "key_id".to_owned())]),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };
    let config = NdcConnectorConfig::for_test(BTreeMap::from([("unrelated".to_owned(), procedures)]));

    assert!(check_key_arguments(&config, &index()).is_ok());
}

#[test]
fn a_mapping_with_no_key_arguments_has_nothing_to_check() {
    let config = keyed_delete(BTreeMap::new());

    assert!(check_key_arguments(&config, &index()).is_ok());
}
