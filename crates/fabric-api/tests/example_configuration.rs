//! The shipped configuration and catalogue must load.
//!
//! An example that has drifted from the code is worse than no example, because
//! people trust it. These fail the build the moment a field is renamed without
//! the example following — exactly when it is cheap to fix.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod examples_support;

use examples_support::{catalog, config};
use fabric_api::config::TokenConfig;
use fabric_core::LogicalResourceName;
use fabric_data_api::OperationKind;

#[test]
fn the_example_configuration_loads_and_validates() {
    let config = config();

    config.validate().expect("the example config must validate");

    assert_eq!(config.listen, "0.0.0.0:8080");
    assert_eq!(config.identity.tenant_claim, "tenant_id");
    assert!(config.identity.reject_tenant_header);
    assert_eq!(config.connectors.len(), 1);
}

#[test]
fn the_example_configuration_uses_the_canonical_identity_posture() {
    // The example is what people copy, so it must demonstrate the architecture
    // rather than an opt-in hardening mode. See ADR 0002.
    assert!(
        matches!(config().token, TokenConfig::TrustedIngress),
        "the example config must ship the canonical trusted-ingress posture"
    );
}

#[test]
fn tenants_and_data_sources_are_configured_as_separate_files() {
    // They are reconciled independently; sharing a file would defeat that.
    let config = config();

    assert_ne!(config.tenants_path, config.data_sources_path);
}

#[test]
fn the_example_connector_declares_valid_write_mappings() {
    let config = config();
    let connector = config.connectors.first().unwrap();

    // `validate` rejects an update or delete mapping with no filter_argument,
    // because the tenant predicate would have nowhere to go.
    connector.validate().unwrap();
    assert!(connector.has_writes());
}

// ----------------------------------------------------------------- catalogue

#[test]
fn the_example_catalogue_parses() {
    let catalog = catalog();

    assert_eq!(catalog.len(), 3);

    let customers = catalog
        .resolve(&LogicalResourceName::try_new("customers").unwrap())
        .unwrap();
    assert_eq!(customers.data_source.as_str(), "primary");
    assert!(customers.allows(OperationKind::Delete));
}

#[test]
fn a_read_only_catalogue_entry_has_not_acquired_write_operations() {
    let catalog = catalog();

    let audit = catalog
        .resolve(&LogicalResourceName::try_new("auditEvents").unwrap())
        .unwrap();

    assert!(!audit.allows(OperationKind::Delete));
}

// -------------------------------------------------------------- data sources
