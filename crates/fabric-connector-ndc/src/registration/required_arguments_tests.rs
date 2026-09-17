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

/// Before `supplied_arguments` was made verb-aware, `payload_argument` was
/// counted as "supplied" for every verb —
/// including delete, which `translate::mutation::to_mutation_request` never
/// reads a payload for at all, because a delete has none. So a mapping that
/// mistakenly wrote its `key_id` value into `payload_argument` instead of
/// `key_arguments` passed this check by coincidence: the connector would
/// still refuse every delete for want of `key_id`, but nothing here said so.
#[test]
fn a_delete_mapping_that_puts_a_key_value_in_payload_argument_instead_of_key_arguments_is_refused() {
    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: Some("key_id".to_owned()),
            filter_argument: Some("pre_check".to_owned()),
            key_arguments: BTreeMap::from([("tenant_key".to_owned(), "key_tenant_key".to_owned())]),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };
    let config = NdcConnectorConfig::for_test(BTreeMap::from([("articles".to_owned(), procedures)]));

    let error = check_required_arguments(&config, &keyed_index()).unwrap_err();

    assert!(error.contains("articles.delete"), "{error}");
    assert!(error.contains("requires `key_id`"), "{error}");
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

/// `pre_check` is nullable and, unlike every other test in this file, left
/// genuinely unsupplied here — `filter_argument` is `None`, not merely absent
/// from `key_arguments` while still reaching the schema some other way. Every
/// other fixture in this file, including `keyed_delete` above, sets
/// `filter_argument = Some("pre_check")`, so a version of this test built on
/// it would pass whether or not nullable arguments were checked at all: the
/// nullable one would be supplied either way, telling this test nothing about
/// what it claims to pin.
#[test]
fn a_nullable_argument_is_never_required() {
    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: None,
            filter_argument: None,
            key_arguments: BTreeMap::from([
                ("id".to_owned(), "key_id".to_owned()),
                ("tenant_key".to_owned(), "key_tenant_key".to_owned()),
            ]),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };
    let config = NdcConnectorConfig::for_test(BTreeMap::from([("articles".to_owned(), procedures)]));

    assert!(check_required_arguments(&config, &keyed_index()).is_ok());
}

/// The filter arm of `supplied_arguments`: the pre-existing
/// `delete_customers(filter)` shape still passes it. `filter` here is a
/// **bare, non-nullable** `predicate` — no `nullable` wrapper — so it is
/// required, and only `filter_argument` can supply it.
///
/// This replaces a version of this test that gave `filter` a `nullable`
/// wrapper. `SchemaIndex::required_arguments` never reports a nullable
/// argument as required at all (see `a_nullable_argument_is_never_required`
/// above), so that version passed whether or not `supplied_arguments` counted
/// `filter_argument` for a delete — it substituted a nullable predicate for
/// the claim it was supposed to pin, and could not have caught the false
/// negative
/// `a_delete_mapping_that_puts_a_key_value_in_payload_argument_instead_of_key_arguments_is_refused`,
/// further above, pins.
#[test]
fn a_delete_mapping_supplying_its_one_required_filter_argument_still_passes() {
    let schema: NdcSchemaResponse = serde_json::from_str(
        r#"{"procedures": [{"name": "delete_customers", "arguments": {
            "filter": {"type": {"type": "predicate", "object_type_name": "customers"}}
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

/// The payload arm of `supplied_arguments`, pinned the same way: a procedure
/// shaped like the real `insert_articles`, whose `objects` argument is a
/// **bare, non-nullable** `array` — required, and only `payload_argument` can
/// supply it. An insert has no filter or key argument to fall back on, so
/// this is the one setting that has to be counted for this verb.
#[test]
fn an_insert_mapping_supplying_its_one_required_payload_argument_is_accepted() {
    let schema: NdcSchemaResponse = serde_json::from_str(
        r#"{"procedures": [{"name": "insert_articles", "arguments": {
            "objects": {"type": {"type": "array", "element_type": {"type": "named", "name": "articles"}}}
        }}]}"#,
    )
    .unwrap();

    let procedures = CollectionProcedures {
        insert: Some(ProcedureBinding {
            procedure: "insert_articles".to_owned(),
            payload_argument: Some("objects".to_owned()),
            filter_argument: None,
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };
    let config = NdcConnectorConfig::for_test(BTreeMap::from([("articles".to_owned(), procedures)]));

    assert!(check_required_arguments(&config, &SchemaIndex::build(&schema)).is_ok());
}

/// The full `articles` mapping against the real, checked-in
/// `schema-named.json` (issue #62's capture) — not a hand-written fixture
/// like `KEYED_DELETE_SCHEMA` above. Exercises all three verbs at once,
/// including the update arm of `supplied_arguments`, which no other test in
/// this file reaches: every other update-shaped check in this crate goes
/// through `translate::mutation_tests`, `config::connector_validation_tests`,
/// or `registration::procedure_arguments_tests` instead.
#[test]
fn the_full_articles_mapping_covers_every_required_argument_on_the_real_schema() {
    let path = format!(
        "{}/tests/fixtures/ndc-postgres-v3.1.0/schema-named.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let schema: NdcSchemaResponse = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let index = SchemaIndex::build(&schema);

    let key_arguments = BTreeMap::from([
        ("id".to_owned(), "key_id".to_owned()),
        ("tenant_key".to_owned(), "key_tenant_key".to_owned()),
    ]);

    let procedures = CollectionProcedures {
        insert: Some(ProcedureBinding {
            procedure: "insert_articles".to_owned(),
            payload_argument: Some("objects".to_owned()),
            filter_argument: None,
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        update: Some(ProcedureBinding {
            procedure: "update_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: Some("update_columns".to_owned()),
            filter_argument: Some("pre_check".to_owned()),
            key_arguments: key_arguments.clone(),
            payload_shape: PayloadShape::SetOperations,
        }),
        delete: Some(ProcedureBinding {
            procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: None,
            filter_argument: Some("pre_check".to_owned()),
            key_arguments,
            payload_shape: PayloadShape::Values,
        }),
    };
    let config = NdcConnectorConfig::for_test(BTreeMap::from([("articles".to_owned(), procedures)]));

    assert!(check_required_arguments(&config, &index).is_ok());
}

#[test]
fn a_read_only_connector_has_nothing_to_check() {
    let config = NdcConnectorConfig::for_test(BTreeMap::new());

    assert!(check_required_arguments(&config, &keyed_index()).is_ok());
}
