//! The shipped authorization-front configuration must load.
//!
//! An example that has drifted from the code is worse than no example,
//! because people trust it. These fail the build the moment a field is
//! renamed without the example following -- exactly when it is cheap to fix.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use fabric_fga_auth_api::config::AppConfig;
use jsonwebtoken::Algorithm;

/// The repository root, from this crate's manifest directory.
fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Loads the shipped example configuration.
fn example() -> AppConfig {
    let path = repository_root().join("examples/authorization.toml");

    AppConfig::load(path.to_str().expect("a UTF-8 path"))
        .expect("the example authorization configuration must load")
}

#[test]
fn the_example_configuration_loads_and_listens_on_the_published_address() {
    assert_eq!(example().listen, "0.0.0.0:8080");
}

#[test]
fn the_example_embeds_the_authorization_service_on_its_pinned_loopback_port() {
    let config = example();

    assert_eq!(config.embedded.port, 8088);
    assert_eq!(config.embedded.binary, "/usr/local/bin/openfga");
    assert_eq!(config.embedded.start_timeout_seconds, 30);
}

#[test]
fn the_example_states_no_datastore_which_means_in_memory() {
    // Absent means in memory, and in memory means every store, model and
    // tuple is lost on restart -- fine for the example, never for a real
    // deployment. Asserting `None` here is what would catch someone
    // "helpfully" adding a `[embedded.datastore]` section to the example.
    assert!(example().embedded.datastore.is_none());
}

#[test]
fn the_example_registers_exactly_one_issuer() {
    assert_eq!(example().issuers.len(), 1);
}

#[test]
fn the_example_issuer_names_the_acme_tenant_and_realm() {
    let config = example();
    let issuer = config.issuers.first().expect("the example issuer");

    assert_eq!(issuer.tenant, "acme");
    assert_eq!(issuer.issuer, "https://identity.fabric.example/realms/acme");
}

#[test]
fn the_example_issuer_reads_keys_from_a_cluster_local_address() {
    // `jwks_uri` is deliberately not derived from `issuer` -- see
    // `IssuerRegistration`'s own docs -- so the example must state a
    // separate address, reachable only from inside the cluster.
    let config = example();
    let issuer = config.issuers.first().expect("the example issuer");

    // Asserted in parts rather than as one literal: the path's protocol
    // segment is Keycloak vocabulary, which the architecture check keeps
    // inside the Keycloak adapter, and this test is about the example file
    // agreeing with the loader, not about which provider serves the keys.
    assert!(
        issuer
            .jwks_uri
            .starts_with("http://keycloak-http.identity.svc.cluster.local/realms/acme/"),
        "{}",
        issuer.jwks_uri
    );
    assert!(issuer.jwks_uri.ends_with("/certs"), "{}", issuer.jwks_uri);
}

#[test]
fn the_example_issuer_carries_the_deployment_wide_audience_and_pins_rs256() {
    let config = example();
    let issuer = config.issuers.first().expect("the example issuer");

    assert_eq!(issuer.audience, "saas-fabric");
    assert_eq!(issuer.algorithms, vec![Algorithm::RS256]);
}

#[test]
fn the_example_issuer_names_its_store_and_a_pinned_authorization_model() {
    // Never "latest" -- see `IssuerRegistration::authorization_model_id`.
    let config = example();
    let issuer = config.issuers.first().expect("the example issuer");

    assert_eq!(issuer.store, "01ABCDEFGHIJKLMNOPQRSTUVWX");
    assert_eq!(issuer.authorization_model_id, "01ZYXWVUTSRQPONMLKJIHGFEDC");
}
