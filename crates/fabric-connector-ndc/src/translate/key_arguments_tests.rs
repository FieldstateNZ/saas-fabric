//! Tests for `key_arguments`.

use std::collections::BTreeMap;

use fabric_connector::{
    CollectionName, ComparisonOperator, ConnectorError, FieldName, Filter, MutationSpec, Row,
};
use serde_json::Value;

use super::key_arguments::*;
use crate::config::{PayloadShape, ProcedureBinding};

fn equal(field: &str, value: &str) -> Filter {
    Filter::Compare {
        field: FieldName::try_new(field).unwrap(),
        operator: ComparisonOperator::Equal,
        value: Value::String(value.to_owned()),
    }
}

fn delete_spec() -> MutationSpec {
    MutationSpec::Delete {
        collection: CollectionName::try_new("articles").unwrap(),
        filter: Some(equal("id", "9")),
    }
}

fn update_spec() -> MutationSpec {
    MutationSpec::Update {
        collection: CollectionName::try_new("articles").unwrap(),
        filter: Some(equal("id", "9")),
        changes: Row::new(),
    }
}

/// `add_key_arguments` used to `insert` a key argument's value
/// unconditionally, so a key argument name colliding with the payload or
/// predicate argument already written into `arguments` would silently
/// overwrite it. Config validation
/// (`NdcConnectorConfig::validate_key_argument_distinctness`) already refuses
/// this shape of mapping at startup, but this test calls `add_key_arguments`
/// directly, bypassing `validate()` entirely — the same "should be
/// impossible, still checked" posture as
/// `translate::mutation::ensure_procedure_accepts`'s re-check of
/// `filter_argument`.
#[test]
fn a_key_argument_colliding_with_the_filter_argument_is_refused_rather_than_overwriting_it() {
    let binding = ProcedureBinding {
        procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
        payload_argument: None,
        filter_argument: Some("pre_check".to_owned()),
        key_arguments: BTreeMap::from([("id".to_owned(), "pre_check".to_owned())]),
        payload_shape: PayloadShape::Values,
    };
    let mut arguments = BTreeMap::from([(
        "pre_check".to_owned(),
        Value::String("the real predicate".to_owned()),
    )]);
    let filter = equal("id", "9");

    let error = add_key_arguments(&mut arguments, &binding, &filter, &delete_spec()).unwrap_err();

    let ConnectorError::InvalidOperation(message) = error else {
        panic!("expected InvalidOperation, got {error:?}");
    };
    assert!(message.contains("`pre_check`"), "{message}");
    assert!(message.contains("filter_argument"), "{message}");

    // The collision is refused, not applied: the predicate already in
    // `arguments` must survive untouched.
    assert_eq!(
        arguments["pre_check"],
        Value::String("the real predicate".to_owned())
    );
}

/// The same guard, exercised against the other setting `arguments` can
/// already hold by the time a key argument is added: the payload, placed
/// there by `procedure_arguments::payload` before `add_predicate` runs.
#[test]
fn a_key_argument_colliding_with_the_payload_argument_is_refused() {
    let binding = ProcedureBinding {
        procedure: "update_articles_by_id_and_tenant_key".to_owned(),
        payload_argument: Some("update_columns".to_owned()),
        filter_argument: Some("pre_check".to_owned()),
        key_arguments: BTreeMap::from([("id".to_owned(), "update_columns".to_owned())]),
        payload_shape: PayloadShape::SetOperations,
    };
    let mut arguments = BTreeMap::from([
        (
            "update_columns".to_owned(),
            Value::String("the real payload".to_owned()),
        ),
        ("pre_check".to_owned(), Value::String("the predicate".to_owned())),
    ]);
    let filter = equal("id", "9");

    let error = add_key_arguments(&mut arguments, &binding, &filter, &update_spec()).unwrap_err();

    let ConnectorError::InvalidOperation(message) = error else {
        panic!("expected InvalidOperation, got {error:?}");
    };
    assert!(message.contains("`update_columns`"), "{message}");
    assert!(message.contains("payload_argument"), "{message}");
    assert_eq!(
        arguments["update_columns"],
        Value::String("the real payload".to_owned())
    );
}

/// The third branch `existing_setting` can report: two key fields mapped to
/// the same argument name, which is its own collision even though neither is
/// the payload or the predicate.
#[test]
fn a_key_argument_colliding_with_another_key_argument_is_refused() {
    let binding = ProcedureBinding {
        procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
        payload_argument: None,
        filter_argument: Some("pre_check".to_owned()),
        key_arguments: BTreeMap::from([
            ("id".to_owned(), "key_id".to_owned()),
            ("tenant_key".to_owned(), "key_id".to_owned()),
        ]),
        payload_shape: PayloadShape::Values,
    };
    let mut arguments = BTreeMap::from([("pre_check".to_owned(), Value::String("filter".to_owned()))]);
    // `BTreeMap` iterates `key_arguments` in field-name order, so `id` is
    // added first and `tenant_key` is the one that finds `key_id` taken.
    let filter = Filter::And {
        clauses: vec![equal("id", "9"), equal("tenant_key", "tenant-482")],
    };

    let error = add_key_arguments(&mut arguments, &binding, &filter, &delete_spec()).unwrap_err();

    let ConnectorError::InvalidOperation(message) = error else {
        panic!("expected InvalidOperation, got {error:?}");
    };
    assert!(message.contains("`key_id`"), "{message}");
    assert!(message.contains("the key argument for field `id`"), "{message}");
}

#[test]
fn a_key_argument_with_nothing_already_at_its_name_is_added() {
    let binding = ProcedureBinding {
        procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
        payload_argument: None,
        filter_argument: Some("pre_check".to_owned()),
        key_arguments: BTreeMap::from([("id".to_owned(), "key_id".to_owned())]),
        payload_shape: PayloadShape::Values,
    };
    let mut arguments = BTreeMap::from([("pre_check".to_owned(), Value::String("filter".to_owned()))]);
    let filter = equal("id", "9");

    add_key_arguments(&mut arguments, &binding, &filter, &delete_spec()).unwrap();

    assert_eq!(arguments["key_id"], Value::String("9".to_owned()));
}
