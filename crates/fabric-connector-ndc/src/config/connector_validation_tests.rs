//! What connector configuration validation protects against.

use std::collections::BTreeMap;

use crate::config::{CollectionProcedures, NdcConnectorConfig, ProcedureBinding};

fn mapping(collection: &str, procedures: CollectionProcedures) -> BTreeMap<String, CollectionProcedures> {
    BTreeMap::from([(collection.to_owned(), procedures)])
}

fn insert_only() -> CollectionProcedures {
    CollectionProcedures {
        insert: Some(ProcedureBinding {
            procedure: "insert_customers".to_owned(),
            payload_argument: Some("objects".to_owned()),
            filter_argument: None,
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
fn a_colliding_delete_mapping_is_rejected_even_though_a_delete_sends_no_payload() {
    // A delete never reads `payload_argument`, so this collision is inert
    // today. It is still incoherent configuration, and letting it start means
    // the mapping is one field away from the update bug with nothing to catch
    // it.
    let procedures = CollectionProcedures {
        delete: Some(ProcedureBinding {
            procedure: "delete_customers".to_owned(),
            payload_argument: Some("filter".to_owned()),
            filter_argument: Some("filter".to_owned()),
        }),
        ..CollectionProcedures::default()
    };

    let error = NdcConnectorConfig::for_test(mapping("customers", procedures))
        .validate()
        .unwrap_err();

    assert!(error.contains("customers.delete"));
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
