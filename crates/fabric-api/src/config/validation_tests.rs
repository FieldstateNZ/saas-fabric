//! The cross-domain checks, and what each one is protecting against.

use std::path::PathBuf;

use fabric_connector_ndc::NdcConnectorConfig;

use crate::config::AppConfig;

fn connector(id: &str) -> NdcConnectorConfig {
    serde_json::from_str(&format!(r#"{{"id":"{id}","endpoint":"http://connector"}}"#)).unwrap()
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
