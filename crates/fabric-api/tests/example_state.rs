//! The shipped tenant bindings and DataSources must be coherent.
//!
//! Beyond parsing, these check the relationships between the three files: every
//! binding names a DataSource that exists, every DataSource names a configured
//! connector, and nothing physical has leaked into the tenant file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod examples_support;

use std::collections::BTreeSet;

use examples_support::{catalog, config, data_sources, raw, tenants};
use fabric_core::LogicalDataSourceName;

#[test]
fn the_example_data_sources_parse_and_validate() {
    let sources = data_sources();

    assert_eq!(sources.len(), 4);

    for source in &sources {
        source.validate().unwrap_or_else(|error| panic!("{error}"));
    }
}

#[test]
fn the_example_data_sources_cover_the_placement_classes_worth_showing() {
    // The examples double as documentation of §17. If a class stops parsing,
    // the example stops teaching it.
    let classes: BTreeSet<&str> = data_sources()
        .iter()
        .map(|source| source.placement.as_str())
        .collect();

    assert!(classes.contains("dedicated"));
    assert!(classes.contains("shared"));
    assert!(classes.contains("regulated"));
}

#[test]
fn a_draining_data_source_can_be_expressed() {
    let sources = data_sources();
    let draining = sources
        .iter()
        .find(|source| !source.capabilities.accepts_new_tenants);

    assert!(
        draining.is_some(),
        "the examples should demonstrate draining a data source"
    );
}

#[test]
fn no_example_data_source_carries_a_credential() {
    // §21: a reference, never a value. A connection string in this file would
    // be a secret checked into the repository.
    let raw = raw("data-sources.json");

    assert!(!raw.contains("password"));
    assert!(!raw.contains("postgres://"));
}

// ------------------------------------------------------------------- tenants

#[test]
fn the_example_tenants_parse_and_validate() {
    let bindings = tenants();

    assert_eq!(bindings.len(), 3);

    for binding in &bindings {
        binding.validate().unwrap_or_else(|error| panic!("{error}"));
    }
}

#[test]
fn the_example_tenants_cover_every_isolation_model() {
    // §18, demonstrated rather than described.
    let models: BTreeSet<&str> = tenants()
        .iter()
        .flat_map(|binding| binding.data.values())
        .map(|data| data.isolation.telemetry_label())
        .collect();

    assert!(models.contains("database"));
    assert!(models.contains("schema"));
    assert!(models.contains("discriminator"));
}

#[test]
fn every_tenant_binding_names_a_data_source_that_exists() {
    // A binding pointing at a missing DataSource fails closed at request time.
    // Catching it here keeps the examples coherent.
    let known: BTreeSet<String> = data_sources()
        .iter()
        .map(|source| source.id.to_string())
        .collect();

    for binding in &tenants() {
        for (logical, data) in &binding.data {
            assert!(
                known.contains(&data.data_source.to_string()),
                "{}.{logical} names data source {}, which data-sources.json does not define",
                binding.tenant,
                data.data_source
            );
        }
    }
}

#[test]
fn every_data_source_names_a_configured_connector() {
    let configured: BTreeSet<String> = config()
        .connectors
        .iter()
        .map(|connector| connector.id.to_string())
        .collect();

    for source in &data_sources() {
        assert!(
            configured.contains(&source.connector.to_string()),
            "data source {} names connector {}, which the example config does not define",
            source.id,
            source.connector
        );
    }
}

#[test]
fn a_tenant_binding_carries_no_physical_configuration() {
    // The model change this PR makes: physical concerns live on the DataSource.
    // A connector or endpoint appearing in the tenant file is the regression to
    // catch.
    let raw = raw("tenants.json");

    assert!(!raw.contains("connector"));
    assert!(!raw.contains("pool"));
    assert!(!raw.contains("endpoint"));
}

#[test]
fn every_catalogued_logical_data_source_is_bound_by_at_least_one_tenant() {
    // A catalogue entry naming a logical source no tenant declares would 500 on
    // its first request.
    let catalog = catalog();

    let bound: BTreeSet<LogicalDataSourceName> = tenants()
        .iter()
        .flat_map(|binding| binding.data.keys().cloned())
        .collect();

    for name in catalog.names() {
        let logical = &catalog.resolve(name).unwrap().data_source;
        assert!(
            bound.contains(logical),
            "catalogue resource {name} needs logical data source {logical}, which no example tenant declares"
        );
    }
}
