//! The cross-domain checks, and what each one is protecting against.

use std::path::PathBuf;

use fabric_connector_ndc::NdcConnectorConfig;

use crate::config::AppConfig;

fn connector(id: &str) -> NdcConnectorConfig {
    serde_json::from_str(&format!(r#"{{"id":"{id}","endpoint":"http://connector"}}"#)).unwrap()
}

fn connector_with_timeout(id: &str, http_timeout_seconds: u64) -> NdcConnectorConfig {
    serde_json::from_str(&format!(
        r#"{{"id":"{id}","endpoint":"http://connector","http_timeout_seconds":{http_timeout_seconds}}}"#
    ))
    .unwrap()
}

fn config_with(connectors: Vec<NdcConnectorConfig>) -> AppConfig {
    AppConfig {
        connectors,
        ..AppConfig::default()
    }
}

#[test]
fn a_configuration_with_no_connectors_is_rejected() {
    // Nothing could be executed, so this is a misconfiguration rather than a
    // deployment that happens to do nothing.
    assert!(AppConfig::default().validate().is_err());
}

#[test]
fn a_single_connector_is_enough() {
    assert!(config_with(vec![connector("postgres")]).validate().is_ok());
}

#[test]
fn duplicate_connector_ids_are_rejected() {
    // One would silently replace the other in the registry, so requests would
    // reach the wrong database with nothing logged.
    let config = config_with(vec![connector("postgres"), connector("postgres")]);

    assert!(config.validate().unwrap_err().contains("configured twice"));
}

#[test]
fn an_invalid_connector_is_rejected_through_its_own_validation() {
    let mut broken = connector("postgres");
    broken.endpoint = "  ".to_owned();

    assert!(config_with(vec![broken]).validate().is_err());
}

#[test]
fn tenants_and_data_sources_may_not_share_a_file() {
    // Each source loads a complete set, so a shared file would have each load
    // parse the other's records and remove everything it did not recognise.
    let config = AppConfig {
        tenants_path: PathBuf::from("/etc/fabric/state.json"),
        data_sources_path: PathBuf::from("/etc/fabric/state.json"),
        ..config_with(vec![connector("postgres")])
    };

    assert!(config.validate().unwrap_err().contains("must differ"));
}

#[test]
fn the_default_paths_already_differ() {
    let config = config_with(vec![connector("postgres")]);

    assert_ne!(config.tenants_path, config.data_sources_path);
}

#[test]
fn the_default_request_timeout_already_covers_the_default_connector_timeout() {
    // The shipped defaults must satisfy the relationship this validation
    // enforces, or every fresh deployment would fail its own startup check.
    let config = config_with(vec![connector("postgres")]);

    assert!(config.validate().is_ok());
}

#[test]
fn a_request_timeout_shorter_than_the_longest_connector_timeout_is_rejected() {
    let config = AppConfig {
        request_timeout_seconds: 5,
        ..config_with(vec![connector_with_timeout("postgres", 10)])
    };

    let error = config.validate().unwrap_err();
    assert!(error.contains("request_timeout_seconds"));
}

#[test]
fn a_request_timeout_exactly_matching_the_longest_connector_timeout_is_accepted() {
    let config = AppConfig {
        request_timeout_seconds: 10,
        ..config_with(vec![connector_with_timeout("postgres", 10)])
    };

    assert!(config.validate().is_ok());
}

#[test]
fn the_longest_of_several_connector_timeouts_is_what_gets_compared() {
    let config = AppConfig {
        request_timeout_seconds: 20,
        ..config_with(vec![
            connector_with_timeout("fast", 5),
            connector_with_timeout("slow", 25),
        ])
    };

    assert!(config.validate().unwrap_err().contains("25"));
}

#[test]
fn a_zero_request_timeout_is_rejected() {
    let config = AppConfig {
        request_timeout_seconds: 0,
        ..config_with(vec![connector("postgres")])
    };

    assert!(config.validate().unwrap_err().contains("request_timeout_seconds"));
}

#[test]
fn a_zero_connector_retry_interval_is_rejected() {
    let config = AppConfig {
        connector_retry_interval_seconds: 0,
        ..config_with(vec![connector("postgres")])
    };

    assert!(config
        .validate()
        .unwrap_err()
        .contains("connector_retry_interval_seconds"));
}

#[test]
fn validate_runs_the_administrator_role_check() {
    // The wiring, not the rule: `administrator_role` has its own tests for
    // what it rejects, and this pins that `validate` actually calls it.
    let mut config = config_with(vec![connector("postgres")]);
    config.permissions.administrator_role = String::new();

    assert!(config.validate().is_err());
}
