//! The model a declaration translates to, as the JSON the service reads.

use fabric_client_model::{AuthorizationConfiguration, DesiredStateError, Relation, ResourceAuthorization};
use fabric_core::{LogicalResourceName, OperationKind, RelationName};
use serde_json::{json, Value};

use super::authorization_model;

fn relation(name: &str, permits: &[OperationKind]) -> Relation {
    Relation {
        name: RelationName::try_new(name).unwrap(),
        permits: permits.to_vec(),
    }
}

fn declaring(resources: Vec<(&str, Vec<Relation>)>) -> AuthorizationConfiguration {
    AuthorizationConfiguration {
        resources: resources
            .into_iter()
            .map(|(name, relations)| ResourceAuthorization {
                resource: LogicalResourceName::try_new(name).unwrap(),
                relations,
            })
            .collect(),
    }
}

/// ADR 0013's own example.
fn customers() -> AuthorizationConfiguration {
    use OperationKind::{Create, Delete, List, Read, Update};

    declaring(vec![(
        "customers",
        vec![
            relation("viewer", &[Read, List]),
            relation("editor", &[Read, List, Create, Update]),
            relation("owner", &[Read, List, Create, Update, Delete]),
        ],
    )])
}

fn model(configuration: &AuthorizationConfiguration) -> Value {
    serde_json::to_value(authorization_model(configuration).unwrap()).unwrap()
}

/// The named type's definition.
fn type_named(model: &Value, name: &str) -> Value {
    model["type_definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|definition| definition["type"] == name)
        .unwrap_or_else(|| panic!("no type {name} in {model}"))
        .clone()
}

/// The relation names a type defines.
fn relations_of(definition: &Value) -> Vec<String> {
    definition["relations"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

fn direct() -> Value {
    json!({"directly_related_user_types": [{"type": "user"}]})
}

#[test]
fn adr_0013s_example_translates_to_a_record_type_and_a_collection_type() {
    assert_eq!(
        model(&customers()),
        json!({
            "schema_version": "1.1",
            "type_definitions": [
                {"type": "user"},
                {
                    "type": "customers",
                    "relations": {
                        "viewer": {"this": {}},
                        "editor": {"this": {}},
                        "owner": {"this": {}},
                        "can_read": {"union": {"child": [
                            {"computedUserset": {"relation": "viewer"}},
                            {"computedUserset": {"relation": "editor"}},
                            {"computedUserset": {"relation": "owner"}}
                        ]}},
                        "can_update": {"union": {"child": [
                            {"computedUserset": {"relation": "editor"}},
                            {"computedUserset": {"relation": "owner"}}
                        ]}},
                        "can_delete": {"computedUserset": {"relation": "owner"}}
                    },
                    "metadata": {"relations": {"viewer": direct(), "editor": direct(), "owner": direct()}}
                },
                {
                    "type": "customers_collection",
                    "relations": {
                        "viewer": {"this": {}},
                        "editor": {"this": {}},
                        "owner": {"this": {}},
                        "can_list": {"union": {"child": [
                            {"computedUserset": {"relation": "viewer"}},
                            {"computedUserset": {"relation": "editor"}},
                            {"computedUserset": {"relation": "owner"}}
                        ]}},
                        "can_create": {"union": {"child": [
                            {"computedUserset": {"relation": "editor"}},
                            {"computedUserset": {"relation": "owner"}}
                        ]}}
                    },
                    "metadata": {"relations": {"viewer": direct(), "editor": direct(), "owner": direct()}}
                }
            ]
        })
    );
}

#[test]
fn a_record_type_defines_no_collection_operation_and_a_collection_type_no_record_operation() {
    let model = model(&customers());
    let record = relations_of(&type_named(&model, "customers"));
    let collection = relations_of(&type_named(&model, "customers_collection"));

    for operation in OperationKind::ALL {
        let relation = operation.computed_relation().to_owned();

        assert_eq!(
            collection.contains(&relation),
            operation.is_collection_operation(),
            "{relation} on the collection"
        );
        assert_eq!(
            record.contains(&relation),
            !operation.is_collection_operation(),
            "{relation} on the record"
        );
    }
}

#[test]
fn the_collection_type_is_the_one_the_runtime_asks_about() {
    let customers_name = LogicalResourceName::try_new("customers").unwrap();
    let asked = fabric_core::CollectionObject::try_new(customers_name).unwrap();

    let model = model(&customers());

    assert_eq!(
        type_named(&model, &asked.type_name())["type"],
        "customers_collection"
    );
}

#[test]
fn an_operation_no_relation_permits_is_defined_and_held_by_nobody() {
    let model = model(&declaring(vec![(
        "auditEvents",
        vec![relation("viewer", &[OperationKind::Read])],
    )]));
    let collection = type_named(&model, "auditEvents_collection");
    let nobody = json!({"difference": {
        "base": {"computedUserset": {"relation": "viewer"}},
        "subtract": {"computedUserset": {"relation": "viewer"}}
    }});

    assert_eq!(collection["relations"]["can_list"], nobody);
    assert_eq!(collection["relations"]["can_create"], nobody);
    assert_eq!(
        type_named(&model, "auditEvents")["relations"]["can_delete"],
        nobody
    );
}

#[test]
fn every_declared_resource_gets_both_types_in_declared_order() {
    let model = model(&declaring(vec![
        ("orders", vec![relation("owner", &[OperationKind::Delete])]),
        ("customers", vec![relation("viewer", &[OperationKind::List])]),
    ]));
    let names: Vec<&str> = model["type_definitions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|definition| definition["type"].as_str().unwrap())
        .collect();

    assert_eq!(
        names,
        [
            "user",
            "orders",
            "orders_collection",
            "customers",
            "customers_collection"
        ]
    );
}

#[test]
fn a_configuration_declaring_nothing_is_a_model_of_principals_only() {
    assert_eq!(
        model(&AuthorizationConfiguration::default()),
        json!({"schema_version": "1.1", "type_definitions": [{"type": "user"}]})
    );
}

#[test]
fn a_declaration_validation_refuses_is_not_translated() {
    let colliding = declaring(vec![
        ("customers", vec![relation("viewer", &[OperationKind::List])]),
        (
            "customers_collection",
            vec![relation("viewer", &[OperationKind::Read])],
        ),
    ]);
    let computed_name = declaring(vec![(
        "customers",
        vec![relation("can_list", &[OperationKind::List])],
    )]);

    for configuration in [colliding, computed_name] {
        assert!(matches!(
            authorization_model(&configuration),
            Err(DesiredStateError::InvalidField { .. })
        ));
    }
}
