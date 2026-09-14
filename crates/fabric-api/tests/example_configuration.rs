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
        matches!(config().token, TokenConfig::TrustedIngress {}),
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
    assert!(customers.allows(OperationKind::Create));
}

/// The example connector now maps `update` and `delete` for `customers` to
/// the real keyed procedures a `ndc-postgres` generates
/// (`update_customers_by_id`, `delete_customers_by_id` in
/// `examples/config.toml`) -- issue #67 closed F3 in
/// `docs/verification.md`'s "Connector acceptance (issue #62)" section and
/// `crates/fabric-ndc-acceptance/docs/CONTEXT.md`: a neutral update/delete
/// can now be expressed against a keyed procedure via `key_arguments`. The
/// catalogue matches deliberately: granting an operation the connector can
/// now serve is no longer a promise this example cannot keep.
#[test]
fn the_example_catalogue_now_allows_the_writes_the_connector_can_serve() {
    let catalog = catalog();

    let customers = catalog
        .resolve(&LogicalResourceName::try_new("customers").unwrap())
        .unwrap();
    assert!(customers.allows(OperationKind::Update));
    assert!(customers.allows(OperationKind::Delete));
}

/// `key_arguments` is what lets a neutral update or delete reach a real
/// `ndc-postgres` keyed procedure at all (issue #67) -- the predicate's own
/// `id` equality is repeated as the procedure's required `key_id` argument.
/// Pinning this against the parsed example config means a rename of either
/// name in `examples/config.toml` fails this test rather than surfacing only
/// as a connector refusal at startup.
#[test]
fn the_example_connectors_customers_mapping_names_its_key_argument() {
    let config = config();
    let connector = config.connectors.first().unwrap();
    let customers = connector.procedures.get("customers").unwrap();

    let update = customers.update.as_ref().unwrap();
    assert_eq!(update.key_arguments.get("id").map(String::as_str), Some("key_id"));

    let delete = customers.delete.as_ref().unwrap();
    assert_eq!(delete.key_arguments.get("id").map(String::as_str), Some("key_id"));
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
