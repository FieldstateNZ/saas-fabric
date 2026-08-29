//! The shipped control-plane configuration and client documents must load.
//!
//! An example that has drifted from the code is worse than no example, because
//! people trust it. These fail the build the moment a field is renamed without
//! the example following — exactly when it is cheap to fix.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use fabric_client_model::ClientDocument;
use fabric_control_plane::OperatorConfig;
use fabric_control_plane_api::config::{ControlPlaneAppConfig, DesiredStateConfig, IdentityProviderConfig};

/// The repository root, from this crate's manifest directory.
fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Loads the shipped example configuration.
fn example() -> ControlPlaneAppConfig {
    let path = repository_root().join("examples/control-plane.toml");

    ControlPlaneAppConfig::load(path.to_str().expect("a UTF-8 path"))
        .expect("the example control-plane configuration must load")
}

#[test]
fn the_example_configuration_loads() {
    let config = example();

    assert_eq!(config.listen, "0.0.0.0:8081");
    assert_eq!(config.request_timeout_seconds, 30);
}

#[test]
fn the_example_states_the_development_posture_with_a_non_empty_allowlist() {
    // The example must be runnable without a cluster (§22), and the OIDC
    // posture cannot be: it needs a realm to verify against. So the example
    // demonstrates the trusted-header posture deliberately, and this test
    // pins that choice rather than accepting whichever posture it drifts to.
    //
    // An empty allowlist would authorise every identity the operator network
    // can authenticate, so the example must not demonstrate one either.
    let OperatorConfig::TrustedHeader { header, allowlist } = example().control_plane.operator else {
        panic!("the example must demonstrate the trusted-header posture; see above");
    };

    assert!(!header.is_empty());
    assert!(!allowlist.is_empty());
}

#[test]
fn the_example_operator_posture_builds() {
    example()
        .control_plane
        .operator
        .build(fabric_control_plane::KeyHolder::empty())
        .expect("the example posture must produce a usable authenticator");
}

#[test]
fn the_example_uses_development_adapters_and_says_so() {
    // The example must be runnable without a cluster (§22). Asserting *which*
    // adapters it names is what stops it quietly acquiring a production
    // credential reference that nobody could satisfy locally.
    assert!(matches!(
        example().desired_state,
        DesiredStateConfig::LocalDirectory { .. }
    ));
    assert!(matches!(
        example().identity_provider,
        IdentityProviderConfig::InMemory
    ));
}

#[test]
fn the_example_client_directory_is_the_one_the_example_documents_live_in() {
    let DesiredStateConfig::LocalDirectory { path } = example().desired_state else {
        panic!("the example must use a local directory");
    };

    assert!(repository_root().join(&path).is_dir(), "{}", path.display());
}

#[test]
fn every_example_client_document_parses() {
    let directory = repository_root().join("examples/clients");
    let mut parsed = 0;

    for entry in std::fs::read_dir(&directory).expect("the example clients directory must exist") {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().is_none_or(|extension| extension != "yaml") {
            continue;
        }

        let text = std::fs::read_to_string(&path).expect("a readable document");
        ClientDocument::parse(&text)
            .unwrap_or_else(|error| panic!("{} does not parse: {error}", path.display()));
        parsed += 1;
    }

    assert!(parsed >= 2, "the examples should show more than one client");
}

#[test]
fn an_example_client_carries_sections_the_control_plane_does_not_model() {
    // The examples are what demonstrate the preservation guarantee. One that
    // held only identity would make the guarantee untestable by inspection.
    let text = std::fs::read_to_string(repository_root().join("examples/clients/acme.yaml"))
        .expect("the acme example must exist");

    assert!(text.contains("features:"));
    assert!(text.contains("data:"));
    assert!(ClientDocument::parse(&text).is_ok());
}

#[test]
fn the_managed_desired_state_mode_needs_nothing_else_stated() {
    // The production mode. It carries no repository, no credential and no
    // identifiers -- that is the point of it, and a deployment must be able to
    // state it without also stating where desired state lives. A mode that
    // still required the fields it exists to remove would be no change at all.
    let config: DesiredStateConfig = serde_json::from_value(serde_json::json!({
        "mode": "managed",
    }))
    .expect("the managed mode must load with nothing else stated");

    assert!(matches!(config, DesiredStateConfig::Managed));
}
