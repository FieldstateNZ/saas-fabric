//! What connector configuration validation protects against.

use std::collections::BTreeMap;

use crate::config::{CollectionProcedures, NdcConnectorConfig, PayloadShape, ProcedureBinding};

fn mapping(collection: &str, procedures: CollectionProcedures) -> BTreeMap<String, CollectionProcedures> {
    BTreeMap::from([(collection.to_owned(), procedures)])
}

fn insert_only() -> CollectionProcedures {
    CollectionProcedures {
        insert: Some(ProcedureBinding {
            procedure: "insert_customers".to_owned(),
            payload_argument: Some("objects".to_owned()),
            filter_argument: None,
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    }
}

fn delete_without_filter() -> CollectionProcedures {
    CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_customers".to_owned(),
            payload_argument: None,
            filter_argument: None,
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    }
}

#[test]
fn a_connector_with_no_procedure_mappings_is_read_only() {
    let config = NdcConnectorConfig::for_test(BTreeMap::new());

    assert!(config.validate().is_ok());
    assert!(!config.has_writes());
}

#[test]
fn a_delete_mapping_without_a_filter_argument_is_rejected_at_startup() {
    // The predicate would have nowhere to go, so a tenant-scoped delete would
    // reach every tenant's rows on that DataSource.
    let config = NdcConnectorConfig::for_test(mapping("customers", delete_without_filter()));

    assert!(config.validate().unwrap_err().contains("filter_argument"));
}

#[test]
fn an_update_mapping_without_a_filter_argument_is_rejected_too() {
    let procedures = CollectionProcedures {
        update: Some(ProcedureBinding {
            procedure: "update_customers".to_owned(),
            payload_argument: Some("changes".to_owned()),
            filter_argument: None,
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    let config = NdcConnectorConfig::for_test(mapping("customers", procedures));

    assert!(config.validate().unwrap_err().contains("filter_argument"));
}

#[test]
fn an_insert_mapping_needs_no_filter_argument() {
    // There is no predicate on an insert; isolation comes from stamping.
    let config = NdcConnectorConfig::for_test(mapping("customers", insert_only()));

    assert!(config.validate().is_ok());
    assert!(config.has_writes());
}

// -- A verb that writes values must say where they go ------------------

#[test]
fn an_insert_mapping_without_a_payload_argument_is_rejected_at_startup() {
    // Unlike a missing filter_argument this fails closed — every insert is
    // refused at translation time. It is still rejected here, because "every
    // write 400s in production" is a bad way to find out about a mapping that
    // could never have worked.
    let procedures = CollectionProcedures {
        insert: Some(ProcedureBinding {
            procedure: "insert_customers".to_owned(),
            payload_argument: None,
            filter_argument: None,
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("customers", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("customers.insert"), "{error}");
    assert!(error.contains("payload_argument"), "{error}");
}

#[test]
fn an_update_mapping_without_a_payload_argument_is_rejected_too() {
    let procedures = CollectionProcedures {
        update: Some(ProcedureBinding {
            procedure: "update_customers".to_owned(),
            payload_argument: None,
            filter_argument: Some("filter".to_owned()),
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("customers", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("payload_argument"), "{error}");
}

#[test]
fn a_delete_mapping_needs_no_payload_argument() {
    // A delete carries no row values, so requiring one would reject a
    // configuration that is complete.
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

    assert!(NdcConnectorConfig::for_test(mapping("customers", procedures))
        .validate()
        .is_ok());
}

// -- Payload and predicate must not share an argument name -------------

#[test]
fn an_update_naming_one_argument_for_both_payload_and_predicate_is_rejected() {
    // The predicate is written into the argument map after the payload, so a
    // shared name discards the caller's field values. The procedure is then
    // invoked with only a predicate, reports success, and changes nothing.
    let procedures = CollectionProcedures {
        update: Some(ProcedureBinding {
            procedure: "update_customers".to_owned(),
            payload_argument: Some("filter".to_owned()),
            filter_argument: Some("filter".to_owned()),
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("customers", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("customers.update"));
    assert!(error.contains("payload_argument"));
}

#[test]
fn a_colliding_insert_mapping_is_rejected_too() {
    // Inserts are outside `predicate_bearing`, which is exactly why the check
    // walks every verb instead.
    let procedures = CollectionProcedures {
        insert: Some(ProcedureBinding {
            procedure: "insert_customers".to_owned(),
            payload_argument: Some("objects".to_owned()),
            filter_argument: Some("objects".to_owned()),
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("customers", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("customers.insert"));
}

#[test]
fn distinct_payload_and_filter_arguments_are_accepted() {
    let procedures = CollectionProcedures {
        update: Some(ProcedureBinding {
            procedure: "update_customers".to_owned(),
            payload_argument: Some("update_columns".to_owned()),
            filter_argument: Some("filter".to_owned()),
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    assert!(NdcConnectorConfig::for_test(mapping("customers", procedures))
        .validate()
        .is_ok());
}

#[test]
fn a_procedure_argument_may_share_a_name_with_a_connection_routing_argument() {
    // Routing values travel in the request's top-level `request_arguments`,
    // not in the procedure's argument map, so the two cannot displace each
    // other. Refusing this would reject a configuration that works.
    let procedures = CollectionProcedures {
        update: Some(ProcedureBinding {
            procedure: "update_customers".to_owned(),
            payload_argument: Some("connection_name".to_owned()),
            filter_argument: Some("connection_string".to_owned()),
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    assert!(NdcConnectorConfig::for_test(mapping("customers", procedures))
        .validate()
        .is_ok());
}

#[test]
fn an_empty_endpoint_is_rejected() {
    let mut config = NdcConnectorConfig::for_test(BTreeMap::new());
    config.endpoint = "  ".to_owned();

    assert!(config.validate().is_err());
}

#[test]
fn a_zero_http_timeout_is_rejected() {
    let mut config = NdcConnectorConfig::for_test(BTreeMap::new());
    config.http_timeout_seconds = 0;

    assert!(config.validate().is_err());
}

#[test]
fn a_zero_connect_timeout_is_rejected() {
    let mut config = NdcConnectorConfig::for_test(BTreeMap::new());
    config.http_connect_timeout_seconds = 0;

    assert!(config.validate().is_err());
}

#[test]
fn a_connect_timeout_longer_than_the_total_timeout_is_rejected() {
    // A connect timeout that outlasts the total timeout could never bind —
    // the total timeout always fires first — so it is rejected as
    // configuration that cannot mean what it says.
    let mut config = NdcConnectorConfig::for_test(BTreeMap::new());
    config.http_timeout_seconds = 5;
    config.http_connect_timeout_seconds = 10;

    assert!(config
        .validate()
        .unwrap_err()
        .contains("http_connect_timeout_seconds"));
}

#[test]
fn a_connect_timeout_equal_to_the_total_timeout_is_accepted() {
    let mut config = NdcConnectorConfig::for_test(BTreeMap::new());
    config.http_timeout_seconds = 5;
    config.http_connect_timeout_seconds = 5;

    assert!(config.validate().is_ok());
}

// -- Keyed procedures (issue #67) ---------------------------------------

/// The two `key_arguments` a real `ndc-postgres` v3.1.0 keyed `articles`
/// procedure needs -- see `docs/decisions/0004-write-support-in-the-first-release.md`'s
/// addendum on F3.
fn articles_key_arguments() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("id".to_owned(), "key_id".to_owned()),
        ("tenant_key".to_owned(), "key_tenant_key".to_owned()),
    ])
}

#[test]
fn an_insert_mapping_with_key_arguments_is_rejected_at_startup() {
    // An insert creates rows; it does not select among existing ones, so it
    // has no key to scope by.
    let procedures = CollectionProcedures {
        insert: Some(ProcedureBinding {
            procedure: "insert_articles".to_owned(),
            payload_argument: Some("objects".to_owned()),
            filter_argument: None,
            key_arguments: articles_key_arguments(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("articles", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("articles.insert"), "{error}");
    assert!(error.contains("key_arguments"), "{error}");
}

#[test]
fn a_key_argument_sharing_a_name_with_the_filter_argument_is_rejected() {
    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: None,
            filter_argument: Some("key_id".to_owned()),
            key_arguments: BTreeMap::from([("id".to_owned(), "key_id".to_owned())]),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("articles", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("articles.delete"), "{error}");
    assert!(error.contains("`key_id`"), "{error}");
}

#[test]
fn two_key_arguments_sharing_a_name_are_rejected() {
    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: None,
            filter_argument: Some("pre_check".to_owned()),
            key_arguments: BTreeMap::from([
                ("id".to_owned(), "key_id".to_owned()),
                ("tenant_key".to_owned(), "key_id".to_owned()),
            ]),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("articles", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("articles.delete"), "{error}");
}

#[test]
fn set_operations_on_an_insert_mapping_is_rejected() {
    // An insert sends an array of row objects, not a per-column operation
    // map -- there is nothing for `_set` to wrap.
    let procedures = CollectionProcedures {
        insert: Some(ProcedureBinding {
            procedure: "insert_articles".to_owned(),
            payload_argument: Some("objects".to_owned()),
            filter_argument: None,
            key_arguments: BTreeMap::new(),
            payload_shape: PayloadShape::SetOperations,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("articles", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("articles.insert"), "{error}");
    assert!(error.contains("payload_shape"), "{error}");
}

#[test]
fn set_operations_on_a_delete_mapping_is_rejected() {
    // A delete has no payload argument at all.
    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: None,
            filter_argument: Some("pre_check".to_owned()),
            key_arguments: articles_key_arguments(),
            payload_shape: PayloadShape::SetOperations,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("articles", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("articles.delete"), "{error}");
    assert!(error.contains("payload_shape"), "{error}");
}

#[test]
fn the_full_articles_shaped_update_mapping_is_accepted() {
    let procedures = CollectionProcedures {
        update: Some(ProcedureBinding {
            procedure: "update_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: Some("update_columns".to_owned()),
            filter_argument: Some("pre_check".to_owned()),
            key_arguments: articles_key_arguments(),
            payload_shape: PayloadShape::SetOperations,
        }),
        ..CollectionProcedures::default()
    };

    assert!(NdcConnectorConfig::for_test(mapping("articles", procedures))
        .validate()
        .is_ok());
}

/// The generic collision case is
/// `an_update_naming_one_argument_for_both_payload_and_predicate_is_rejected`,
/// above; this pins `validate_distinct_arguments` against the real, keyed
/// shape this section otherwise exercises, rather than only the hand-written
/// `customers` one.
#[test]
fn a_colliding_update_mapping_is_rejected_on_the_articles_shaped_procedure_too() {
    let procedures = CollectionProcedures {
        update: Some(ProcedureBinding {
            procedure: "update_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: Some("pre_check".to_owned()),
            filter_argument: Some("pre_check".to_owned()),
            key_arguments: articles_key_arguments(),
            payload_shape: PayloadShape::SetOperations,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("articles", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("articles.update"), "{error}");
    assert!(error.contains("payload_argument"), "{error}");
}

#[test]
fn a_delete_mapping_declaring_a_payload_argument_is_rejected_at_startup() {
    // A delete carries no payload at all; naming one is the same mistake that
    // produces `required_arguments`'s false negative -- a `key_id` value
    // written into `payload_argument` instead of `key_arguments`, where
    // translation never sends it for a delete.
    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: Some("accidental_payload".to_owned()),
            filter_argument: Some("pre_check".to_owned()),
            key_arguments: articles_key_arguments(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("articles", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("articles.delete"), "{error}");
    assert!(error.contains("payload_argument"), "{error}");
}

#[test]
fn the_full_articles_shaped_delete_mapping_is_accepted() {
    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
            payload_argument: None,
            filter_argument: Some("pre_check".to_owned()),
            key_arguments: articles_key_arguments(),
            payload_shape: PayloadShape::Values,
        }),
        ..CollectionProcedures::default()
    };

    assert!(NdcConnectorConfig::for_test(mapping("articles", procedures))
        .validate()
        .is_ok());
}
