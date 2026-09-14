//! Mapping neutral writes onto procedure calls.

use std::collections::BTreeMap;

use fabric_connector::{
    CollectionName, ComparisonOperator, ConnectorError, FieldName, Filter, MutationSpec, Row,
};
use serde_json::Value;

use crate::config::{CollectionProcedures, PayloadShape, ProcedureBinding};
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
        key_arguments: BTreeMap::new(),
        payload_shape: PayloadShape::Values,
    }
}

fn update_binding() -> ProcedureBinding {
    ProcedureBinding {
        procedure: "update_customers".to_owned(),
        payload_argument: Some("update_columns".to_owned()),
        filter_argument: Some("filter".to_owned()),
        key_arguments: BTreeMap::new(),
        payload_shape: PayloadShape::Values,
    }
}

fn delete_binding() -> ProcedureBinding {
    ProcedureBinding {
        procedure: "delete_customers".to_owned(),
        payload_argument: None,
        filter_argument: Some("filter".to_owned()),
        key_arguments: BTreeMap::new(),
        payload_shape: PayloadShape::Values,
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
fn a_filter_argument_the_procedure_never_declares_is_refused_at_translation_too() {
    // Startup validation refuses this as well. Both checks are deliberate: the
    // predicate would otherwise go out under a name the procedure never
    // declared, and a connector that ignores unknown arguments would run the
    // delete against every tenant's rows.
    let config = config_with(CollectionProcedures {
        delete: Some(ProcedureBinding {
            filter_argument: Some("where".to_owned()),
            ..delete_binding()
        }),
        ..CollectionProcedures::default()
    });

    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: Some(tenant_predicate()),
    };

    assert!(matches!(
        to_mutation_request(&spec, None, &config, &index()).unwrap_err(),
        ConnectorError::InvalidOperation(_)
    ));
}

#[test]
fn a_filter_argument_naming_a_non_predicate_argument_is_refused() {
    // `update_columns` exists on the procedure, so a name-only check passes
    // this. It is typed as an object, and a predicate sent there is inert.
    let config = update_config(ProcedureBinding {
        payload_argument: Some("filter".to_owned()),
        filter_argument: Some("update_columns".to_owned()),
        ..update_binding()
    });

    assert!(matches!(
        to_mutation_request(&update_spec(), None, &config, &index()).unwrap_err(),
        ConnectorError::InvalidOperation(_)
    ));
}

/// A schema mirroring the real `ndc-postgres` `articles` procedures observed
/// for issue #62 — see
/// `tests/fixtures/ndc-postgres-v3.1.0/schema-static.json`. Separate from
/// [`index`] (the pre-existing `customers` fixture used above), because the
/// two tests below exist specifically to pin this adapter's request against
/// the real connector's own naming, not against this file's synthetic one.
fn articles_index() -> SchemaIndex {
    let schema: NdcSchemaResponse = serde_json::from_str(
        r#"{
            "procedures": [
                {"name": "insert_articles", "arguments": {
                    "objects": {"type": {"type": "array", "element_type": {"type": "named", "name": "articles"}}}
                }}
            ]
        }"#,
    )
    .unwrap();

    SchemaIndex::build(&schema)
}

/// Reads a real `ndc-postgres` v3.1.0 request, checked in under
/// `tests/fixtures/` -- see the README there for how it was captured.
fn fixture(name: &str) -> serde_json::Value {
    let path = format!(
        "{}/tests/fixtures/ndc-postgres-v3.1.0/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// F1, pinned against the real connector's own accepted request rather than a
/// guess: `tests/fixtures/ndc-postgres-v3.1.0/request-insert-affected-only.json`,
/// extracted from the second `curl` in the plan's `probe6.sh`, the request
/// that produced `mutation-insert-affected-only.json`. This is also the
/// shape every insert now takes, `returning` or not — see
/// `NdcMutationFields::affected_rows_only`'s rustdoc for why `MutationSpec`
/// leaves this adapter no way to ask for the other observed shape instead.
#[test]
fn an_insert_request_matches_the_real_connectors_accepted_shape() {
    let config = config_with(CollectionProcedures {
        insert: Some(ProcedureBinding {
            procedure: "insert_articles".to_owned(),
            payload_argument: Some("objects".to_owned()),
            filter_argument: None,
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    });

    let row = Row::new()
        .with(FieldName::try_new("id").unwrap(), Value::String("10".to_owned()))
        .with(
            FieldName::try_new("tenant_key").unwrap(),
            Value::String("tenant-acme-482".to_owned()),
        )
        .with(
            FieldName::try_new("title").unwrap(),
            Value::String("Second".to_owned()),
        )
        .with(FieldName::try_new("body").unwrap(), Value::Null);

    let spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![row],
    };

    let request = to_mutation_request(&spec, None, &config, &articles_index()).unwrap();
    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json, fixture("request-insert-affected-only.json"));
}

/// The bug this pins: `fields` used to be unconditionally `None`
/// (`src/translate/mutation.rs`, before this commit), and a real connector
/// refuses every procedure request that omits it — see
/// `wire::mutation::NdcMutationOperation::Procedure`'s rustdoc. No
/// `MutationSpec` variant this translator handles may reach the connector
/// with no selection.
#[test]
fn every_mutation_variant_selects_fields_rather_than_asking_for_nothing() {
    let insert_config = config_with(CollectionProcedures {
        insert: Some(insert_binding()),
        ..CollectionProcedures::default()
    });
    let insert_spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![Row::new()],
    };
    let insert_request = to_mutation_request(&insert_spec, None, &insert_config, &index()).unwrap();
    let NdcMutationOperation::Procedure { fields, .. } = insert_request.operations.first().unwrap();
    assert!(fields.is_some(), "insert must select fields");

    let update_request =
        to_mutation_request(&update_spec(), None, &update_config(update_binding()), &index()).unwrap();
    let NdcMutationOperation::Procedure { fields, .. } = update_request.operations.first().unwrap();
    assert!(fields.is_some(), "update must select fields");

    let delete_config = config_with(CollectionProcedures {
        delete: Some(delete_binding()),
        ..CollectionProcedures::default()
    });
    let delete_spec = MutationSpec::Delete {
        collection: collection(),
        filter: Some(tenant_predicate()),
    };
    let delete_request = to_mutation_request(&delete_spec, None, &delete_config, &index()).unwrap();
    let NdcMutationOperation::Procedure { fields, .. } = delete_request.operations.first().unwrap();
    assert!(fields.is_some(), "delete must select fields");
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

// -- Keyed procedures (issue #67) --------------------------------------
//
// `ndc-postgres` v3.1.0 generates update and delete procedures keyed by
// primary key rather than accepting a bare predicate — see
// `docs/decisions/0004-write-support-in-the-first-release.md`'s addendum on
// F3. Everything below exercises `key_arguments` and `payload_shape` against
// a schema shaped like that real connector's `articles` collection.

fn articles_collection() -> CollectionName {
    CollectionName::try_new("articles").unwrap()
}

fn articles_config(procedures: CollectionProcedures) -> NdcConnectorConfig {
    NdcConnectorConfig::for_test(BTreeMap::from([("articles".to_owned(), procedures)]))
}

/// A schema mirroring the real `ndc-postgres` v3.1.0 **keyed** `articles`
/// procedures observed for issue #67 — see
/// `tests/fixtures/ndc-postgres-v3.1.0/schema-named.json`. `key_id` and
/// `key_tenant_key` are plain, non-nullable `text` arguments; `pre_check` and
/// `post_check` are nullable predicates, exactly as that schema declares them.
fn keyed_articles_index() -> SchemaIndex {
    let schema: NdcSchemaResponse = serde_json::from_str(
        r#"{
            "scalar_types": {"text": {"comparison_operators": {"_eq": {"type": "equal"}}}},
            "object_types": {"articles": {"fields": {
                "id": {"type": {"type": "named", "name": "text"}},
                "tenant_key": {"type": {"type": "named", "name": "text"}},
                "title": {"type": {"type": "named", "name": "text"}}
            }}},
            "collections": [{"name": "articles", "type": "articles"}],
            "procedures": [
                {"name": "delete_articles_by_id_and_tenant_key", "arguments": {
                    "key_id": {"type": {"type": "named", "name": "text"}},
                    "key_tenant_key": {"type": {"type": "named", "name": "text"}},
                    "pre_check": {"type": {"type": "nullable", "underlying_type":
                        {"type": "predicate", "object_type_name": "articles"}}}
                }},
                {"name": "update_articles_by_id_and_tenant_key", "arguments": {
                    "key_id": {"type": {"type": "named", "name": "text"}},
                    "key_tenant_key": {"type": {"type": "named", "name": "text"}},
                    "update_columns": {"type": {"type": "named",
                        "name": "update_articles_by_id_and_tenant_key_update_columns"}},
                    "pre_check": {"type": {"type": "nullable", "underlying_type":
                        {"type": "predicate", "object_type_name": "articles"}}},
                    "post_check": {"type": {"type": "nullable", "underlying_type":
                        {"type": "predicate", "object_type_name": "articles"}}}
                }}
            ]
        }"#,
    )
    .unwrap();

    SchemaIndex::build(&schema)
}

fn keyed_delete_binding() -> ProcedureBinding {
    ProcedureBinding {
        procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
        payload_argument: None,
        filter_argument: Some("pre_check".to_owned()),
        key_arguments: BTreeMap::from([
            ("id".to_owned(), "key_id".to_owned()),
            ("tenant_key".to_owned(), "key_tenant_key".to_owned()),
        ]),
        payload_shape: PayloadShape::Values,
    }
}

fn keyed_update_binding() -> ProcedureBinding {
    ProcedureBinding {
        procedure: "update_articles_by_id_and_tenant_key".to_owned(),
        payload_argument: Some("update_columns".to_owned()),
        filter_argument: Some("pre_check".to_owned()),
        key_arguments: BTreeMap::from([
            ("id".to_owned(), "key_id".to_owned()),
            ("tenant_key".to_owned(), "key_tenant_key".to_owned()),
        ]),
        payload_shape: PayloadShape::SetOperations,
    }
}

/// A filter shaped like what `MutationSpec::for_target` leaves behind: the
/// primary key's two columns, both equalities, flattened into one `And`.
fn keyed_filter(id: &str, tenant_key: &str) -> Filter {
    Filter::And {
        clauses: vec![
            Filter::Compare {
                field: FieldName::try_new("id").unwrap(),
                operator: ComparisonOperator::Equal,
                value: Value::String(id.to_owned()),
            },
            Filter::Compare {
                field: FieldName::try_new("tenant_key").unwrap(),
                operator: ComparisonOperator::Equal,
                value: Value::String(tenant_key.to_owned()),
            },
        ],
    }
}

/// The real request the connector accepted for a keyed delete —
/// `tests/fixtures/ndc-postgres-v3.1.0/request-delete-other-tenant.json`,
/// extracted verbatim from the plan's `probe6.sh`. Its `pre_check` is a
/// hand-built cross-tenant probe: the key names tenant `acme-482`, but the
/// predicate names tenant `globex-915` — deliberately mismatched, to prove
/// the delete touched nothing (`affected_rows: 0`, in the paired response
/// capture) even when the permission predicate disagreed with the key. This
/// crate's own translation can never reproduce that mismatch, because it
/// reads both the key values and the predicate off the *same* filter — so
/// this test does not compare the whole body. It pins what does not depend on
/// that: the argument names, that both key values came through, and the
/// `fields` selection.
#[test]
fn a_keyed_delete_matches_the_real_connectors_argument_shape() {
    let config = articles_config(CollectionProcedures {
        delete: Some(keyed_delete_binding()),
        ..CollectionProcedures::default()
    });

    let spec = MutationSpec::Delete {
        collection: articles_collection(),
        filter: Some(keyed_filter("9", "tenant-acme-482")),
    };

    let request = to_mutation_request(&spec, None, &config, &keyed_articles_index()).unwrap();
    let NdcMutationOperation::Procedure {
        name,
        arguments,
        fields,
    } = request.operations.first().unwrap();

    let expected = fixture("request-delete-other-tenant.json");
    let expected_operation = &expected["operations"][0];
    let expected_arguments = expected_operation["arguments"].as_object().unwrap();

    assert_eq!(name, "delete_articles_by_id_and_tenant_key");
    assert_eq!(
        arguments.keys().collect::<std::collections::BTreeSet<_>>(),
        expected_arguments
            .keys()
            .collect::<std::collections::BTreeSet<_>>(),
        "argument names must match the real connector's accepted shape"
    );
    assert!(arguments["key_id"].is_string(), "key_id must be sent");
    assert!(
        arguments["key_tenant_key"].is_string(),
        "key_tenant_key must be sent"
    );
    assert_eq!(
        serde_json::to_value(fields).unwrap(),
        expected_operation["fields"],
        "fields selection must match the real connector's accepted shape"
    );
}

#[test]
fn a_keyed_delete_carries_both_key_values_and_the_full_predicate() {
    let config = articles_config(CollectionProcedures {
        delete: Some(keyed_delete_binding()),
        ..CollectionProcedures::default()
    });

    let spec = MutationSpec::Delete {
        collection: articles_collection(),
        filter: Some(keyed_filter("9", "tenant-acme-482")),
    };

    let request = to_mutation_request(&spec, None, &config, &keyed_articles_index()).unwrap();
    let NdcMutationOperation::Procedure { arguments, .. } = request.operations.first().unwrap();

    assert_eq!(arguments["key_id"], "9");
    assert_eq!(arguments["key_tenant_key"], "tenant-acme-482");
    // The tenant discriminator reaches the connector twice — once as a key
    // argument, once inside `pre_check` — by design; see
    // `ProcedureBinding::key_arguments`'s rustdoc.
    assert_eq!(arguments["pre_check"]["type"], "and");
}

/// `payload_shape: set_operations` wraps every field the caller changed —
/// including the stamped discriminator — in `{"_set": value}`, which is what
/// `ndc-postgres`'s `update_columns` argument expects on a keyed update
/// procedure (`update_articles_by_id_and_tenant_key_update_columns`, in
/// `schema-named.json`).
#[test]
fn a_keyed_update_shapes_its_payload_as_set_operations() {
    let config = articles_config(CollectionProcedures {
        update: Some(keyed_update_binding()),
        ..CollectionProcedures::default()
    });

    let spec = MutationSpec::Update {
        collection: articles_collection(),
        filter: Some(keyed_filter("9", "tenant-acme-482")),
        changes: Row::new()
            .with(
                FieldName::try_new("title").unwrap(),
                Value::String("Updated".to_owned()),
            )
            .with(
                FieldName::try_new("tenant_key").unwrap(),
                Value::String("tenant-acme-482".to_owned()),
            ),
    };

    let request = to_mutation_request(&spec, None, &config, &keyed_articles_index()).unwrap();
    let NdcMutationOperation::Procedure { arguments, .. } = request.operations.first().unwrap();

    assert_eq!(
        arguments["update_columns"]["title"],
        serde_json::json!({"_set": "Updated"})
    );
    assert_eq!(
        arguments["update_columns"]["tenant_key"],
        serde_json::json!({"_set": "tenant-acme-482"})
    );
    assert_eq!(arguments["key_id"], "9");
    assert_eq!(arguments["key_tenant_key"], "tenant-acme-482");
}

#[test]
fn a_keyed_delete_missing_a_key_equality_is_refused() {
    let config = articles_config(CollectionProcedures {
        delete: Some(keyed_delete_binding()),
        ..CollectionProcedures::default()
    });

    // Only `id` has an equality; `tenant_key` — the discriminator itself —
    // has none, so the key argument mapped to it has nothing to read.
    let spec = MutationSpec::Delete {
        collection: articles_collection(),
        filter: Some(Filter::Compare {
            field: FieldName::try_new("id").unwrap(),
            operator: ComparisonOperator::Equal,
            value: Value::String("9".to_owned()),
        }),
    };

    let error = to_mutation_request(&spec, None, &config, &keyed_articles_index()).unwrap_err();

    assert!(matches!(error, ConnectorError::InvalidOperation(_)));
}

#[test]
fn a_keyed_delete_with_the_key_only_reachable_through_an_or_is_refused() {
    let config = articles_config(CollectionProcedures {
        delete: Some(keyed_delete_binding()),
        ..CollectionProcedures::default()
    });

    // `id` only holds along one branch of the `Or`, so it is not a key value
    // — treating it as one would let either of two rows be reached when only
    // one was named.
    let filter = Filter::And {
        clauses: vec![
            Filter::Or {
                clauses: vec![
                    Filter::Compare {
                        field: FieldName::try_new("id").unwrap(),
                        operator: ComparisonOperator::Equal,
                        value: Value::String("9".to_owned()),
                    },
                    Filter::Compare {
                        field: FieldName::try_new("id").unwrap(),
                        operator: ComparisonOperator::Equal,
                        value: Value::String("10".to_owned()),
                    },
                ],
            },
            Filter::Compare {
                field: FieldName::try_new("tenant_key").unwrap(),
                operator: ComparisonOperator::Equal,
                value: Value::String("tenant-acme-482".to_owned()),
            },
        ],
    };
    let spec = MutationSpec::Delete {
        collection: articles_collection(),
        filter: Some(filter),
    };

    let error = to_mutation_request(&spec, None, &config, &keyed_articles_index()).unwrap_err();

    assert!(matches!(error, ConnectorError::InvalidOperation(_)));
}

#[test]
fn a_keyed_delete_with_contradictory_key_equalities_is_refused() {
    let config = articles_config(CollectionProcedures {
        delete: Some(keyed_delete_binding()),
        ..CollectionProcedures::default()
    });

    let filter = Filter::And {
        clauses: vec![
            Filter::Compare {
                field: FieldName::try_new("id").unwrap(),
                operator: ComparisonOperator::Equal,
                value: Value::String("9".to_owned()),
            },
            Filter::Compare {
                field: FieldName::try_new("id").unwrap(),
                operator: ComparisonOperator::Equal,
                value: Value::String("10".to_owned()),
            },
            Filter::Compare {
                field: FieldName::try_new("tenant_key").unwrap(),
                operator: ComparisonOperator::Equal,
                value: Value::String("tenant-acme-482".to_owned()),
            },
        ],
    };
    let spec = MutationSpec::Delete {
        collection: articles_collection(),
        filter: Some(filter),
    };

    let error = to_mutation_request(&spec, None, &config, &keyed_articles_index()).unwrap_err();

    assert!(matches!(error, ConnectorError::InvalidOperation(_)));
}

#[test]
fn a_keyed_delete_with_an_in_of_one_value_for_the_key_is_not_accepted() {
    let config = articles_config(CollectionProcedures {
        delete: Some(keyed_delete_binding()),
        ..CollectionProcedures::default()
    });

    // A single-value `In` is the same predicate as an equality, but this
    // crate keeps the key extraction strict rather than treating it as one.
    let filter = Filter::And {
        clauses: vec![
            Filter::In {
                field: FieldName::try_new("id").unwrap(),
                values: vec![Value::String("9".to_owned())],
            },
            Filter::Compare {
                field: FieldName::try_new("tenant_key").unwrap(),
                operator: ComparisonOperator::Equal,
                value: Value::String("tenant-acme-482".to_owned()),
            },
        ],
    };
    let spec = MutationSpec::Delete {
        collection: articles_collection(),
        filter: Some(filter),
    };

    let error = to_mutation_request(&spec, None, &config, &keyed_articles_index()).unwrap_err();

    assert!(matches!(error, ConnectorError::InvalidOperation(_)));
}
