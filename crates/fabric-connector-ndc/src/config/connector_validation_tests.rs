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
