//! Locating and reading the shipped example files.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod probe_connector;
pub mod stub_connector;

use std::path::PathBuf;

use fabric_api::config::AppConfig;
use fabric_data_api::ResourceCatalog;
use fabric_tenant_runtime::{DataSource, TenantRuntimeBinding};

/// Resolves a path inside the workspace `examples/` directory.
pub fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
}

/// Loads the example configuration, panicking with a useful message if it has
/// drifted from the code.
pub fn config() -> AppConfig {
    AppConfig::load(example("config.toml").to_str().unwrap()).expect("the example config must load")
}

/// Reads and parses one of the example JSON files.
pub fn read_json<T: serde::de::DeserializeOwned>(name: &str) -> Vec<T> {
    let contents = std::fs::read_to_string(example(name)).unwrap();

    serde_json::from_str(&contents).unwrap_or_else(|error| panic!("{name} must parse: {error}"))
}

/// The example tenant bindings.
pub fn tenants() -> Vec<TenantRuntimeBinding> {
    read_json("tenants.json")
}

/// The example DataSources.
pub fn data_sources() -> Vec<DataSource> {
    read_json("data-sources.json")
}

/// The example resource catalogue.
pub fn catalog() -> ResourceCatalog {
    let contents = std::fs::read_to_string(example("catalog.json")).unwrap();

    serde_json::from_str(&contents).expect("the catalogue must parse")
}

/// The raw text of an example file, for checks about what must *not* appear.
pub fn raw(name: &str) -> String {
    std::fs::read_to_string(example(name)).unwrap()
}
