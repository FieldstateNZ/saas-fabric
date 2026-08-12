//! Proves the shipped example configuration actually loads.
//!
//! An example that has drifted from the code is worse than no example, because
//! people trust it. These tests fail the build the moment a config field is
//! renamed without the example following — which is exactly when it is cheap to
//! fix.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use fabric_api::config::{AppConfig, TokenConfig};
use fabric_data_api::ResourceCatalog;
use fabric_tenant_runtime::TenantRuntimeBinding;

/// Resolves a path inside the workspace `examples/` directory.
fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

#[test]
fn the_example_configuration_loads_and_validates() {
    let config = AppConfig::load(example("config.toml").to_str().unwrap())
        .expect("the shipped example config must load");

    config
        .validate()
        .expect("the shipped example config must validate");

    assert_eq!(config.listen, "0.0.0.0:8080");
    assert_eq!(config.identity.tenant_claim, "tenant_id");
    assert!(config.identity.reject_tenant_header);
    assert_eq!(config.connectors.len(), 1);
}

#[test]
fn the_example_configuration_verifies_token_signatures() {
    // The example is what people copy. It must not ship the unverified posture.
    let config = AppConfig::load(example("config.toml").to_str().unwrap()).unwrap();

    assert!(
        matches!(config.token, TokenConfig::Validating { .. }),
        "the example config must demonstrate the recommended posture"
    );
}

#[test]
fn the_example_connector_declares_valid_write_mappings() {
    let config = AppConfig::load(example("config.toml").to_str().unwrap()).unwrap();
    let connector = config.connectors.first().unwrap();

    // `validate` rejects an update or delete mapping with no filter_argument,
    // because the tenant predicate would have nowhere to go.
    connector.validate().unwrap();
    assert!(connector.has_writes());
}

#[test]
fn the_example_catalogue_parses() {
    let contents = std::fs::read_to_string(example("catalog.json")).unwrap();
    let catalog: ResourceCatalog = serde_json::from_str(&contents).expect("the example catalogue must parse");

    assert_eq!(catalog.len(), 3);

    let customers = catalog
        .resolve(&fabric_core::LogicalResourceName::try_new("customers").unwrap())
        .unwrap();
    assert_eq!(customers.data_source.as_str(), "primary");
    assert!(customers.allows(fabric_data_api::OperationKind::Delete));

    // A read-only resource must not have acquired write operations by accident.
    let audit = catalog
        .resolve(&fabric_core::LogicalResourceName::try_new("auditEvents").unwrap())
        .unwrap();
    assert!(!audit.allows(fabric_data_api::OperationKind::Delete));
}

#[test]
fn the_example_bindings_parse() {
    let contents = std::fs::read_to_string(example("bindings.json")).unwrap();
    let bindings: Vec<TenantRuntimeBinding> =
        serde_json::from_str(&contents).expect("the example bindings must parse");

    assert_eq!(bindings.len(), 3);
}

#[test]
fn the_example_bindings_cover_every_isolation_model() {
    // The examples are also documentation of §18. If one placement stops
    // parsing, the example stops teaching it.
    let contents = std::fs::read_to_string(example("bindings.json")).unwrap();
    let bindings: Vec<TenantRuntimeBinding> = serde_json::from_str(&contents).unwrap();

    let primary = fabric_core::DataSourceName::try_new("primary").unwrap();

    let models: Vec<&'static str> = bindings
        .iter()
        .filter_map(|binding| binding.data_source(&primary).ok())
        .map(|data| data.isolation.telemetry_label())
        .collect();

    assert!(models.contains(&"database"));
    assert!(models.contains(&"schema"));
}

#[test]
fn every_example_binding_resolves_to_an_execution_target() {
    let contents = std::fs::read_to_string(example("bindings.json")).unwrap();
    let bindings: Vec<TenantRuntimeBinding> = serde_json::from_str(&contents).unwrap();

    let primary = fabric_core::DataSourceName::try_new("primary").unwrap();

    for binding in &bindings {
        let target = binding
            .execution_target(&primary)
            .unwrap_or_else(|error| panic!("{} has no usable primary binding: {error}", binding.tenant));

        assert_eq!(target.tenant(), &binding.tenant);
        assert_eq!(target.revision(), binding.revision);
    }
}

#[test]
fn example_bindings_reference_only_configured_connectors() {
    // A binding naming a connector the process was not started with fails
    // closed at request time. Catching it here keeps the examples coherent.
    let config = AppConfig::load(example("config.toml").to_str().unwrap()).unwrap();
    let configured: Vec<String> = config.connectors.iter().map(|c| c.id.to_string()).collect();

    let contents = std::fs::read_to_string(example("bindings.json")).unwrap();
    let bindings: Vec<TenantRuntimeBinding> = serde_json::from_str(&contents).unwrap();

    for binding in &bindings {
        for (name, data) in &binding.data {
            assert!(
                configured.contains(&data.connector.to_string()),
                "{}.{name} names connector {}, which the example config does not define",
                binding.tenant,
                data.connector
            );
        }
    }
}
