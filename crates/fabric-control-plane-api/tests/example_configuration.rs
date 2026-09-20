//! The shipped control-plane configuration and client documents must load.
//!
//! An example that has drifted from the code is worse than no example, because
//! people trust it. These fail the build the moment a field is renamed without
//! the example following — exactly when it is cheap to fix.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use fabric_client_model::{ClientDocument, DataIntent, API_VERSION, API_VERSION_V2};
use fabric_control_plane::OperatorConfig;
use fabric_control_plane_api::config::{
    ControlPlaneAppConfig, DesiredStateConfig, IdentityProviderConfig, PlatformManagementConfig,
};
use fabric_core::LogicalDataSourceName;
use fabric_platform_management::PlacementClassDocument;

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
fn the_example_states_the_only_posture_completely() {
    // There is one posture and no development shortcut beside it. What this
    // pins is that the example states every field it needs — a blank issuer
    // matches a token from anywhere, and a blank role is held by everybody.
    let OperatorConfig::Oidc {
        issuer,
        client_id,
        required_role,
        redirect_uri,
        ..
    } = example().control_plane.operator;

    for (name, value) in [
        ("issuer", &issuer),
        ("client_id", &client_id),
        ("required_role", &required_role),
        ("redirect_uri", &redirect_uri),
    ] {
        assert!(!value.trim().is_empty(), "the example must state {name}");
    }
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
    let mut versions = std::collections::BTreeSet::new();

    for entry in std::fs::read_dir(&directory).expect("the example clients directory must exist") {
        let path = entry.expect("a readable directory entry").path();
        if path.extension().is_none_or(|extension| extension != "yaml") {
            continue;
        }

        let text = std::fs::read_to_string(&path).expect("a readable document");
        let document = ClientDocument::parse(&text)
            .unwrap_or_else(|error| panic!("{} does not parse: {error}", path.display()));
        parsed += 1;
        versions.insert(document.api_version());
    }

    assert!(
        parsed >= 3,
        "the examples should include at least three client documents"
    );

    // Both schema versions, because the `v1` migrator is only exercised by the
    // shipped corpus if a shipped document actually reaches it.
    assert_eq!(
        versions,
        [API_VERSION, API_VERSION_V2].into_iter().collect(),
        "the examples should cover both schema versions"
    );
}

#[test]
fn an_example_client_carries_sections_the_control_plane_does_not_model() {
    // The examples are what demonstrate the preservation guarantee. One that
    // held only identity would make the guarantee untestable by inspection.
    // `data` is not in this list any more -- ADR 0023 part 2 made it typed,
    // and the test below exercises that through the real loader instead.
    let text = std::fs::read_to_string(repository_root().join("examples/clients/acme.yaml"))
        .expect("the acme example must exist");

    assert!(text.contains("features:"));
    assert!(ClientDocument::parse(&text).is_ok());
}

#[test]
fn an_example_clients_data_intent_reaches_the_typed_model() {
    // The acme example's `spec.data.primary` is what pins `DataIntent`
    // parsing against a real, shipped document rather than only an inline
    // fixture.
    let text = std::fs::read_to_string(repository_root().join("examples/clients/acme.yaml"))
        .expect("the acme example must exist");

    let client = ClientDocument::parse(&text)
        .expect("the acme example parses")
        .into_client();

    let primary = client
        .data
        .get(&LogicalDataSourceName::try_new("primary").expect("a valid logical data source name"))
        .expect("the acme example declares a primary data intent");

    assert_eq!(
        *primary,
        DataIntent {
            class: PlacementClassDocument::Dedicated,
            provider: Some("sql".to_owned()),
            region: Some("au-east".to_owned()),
        }
    );
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

#[test]
fn a_platform_management_section_with_a_publication_block_parses() {
    // The example ships this section commented out (no cluster to publish
    // to), so this is the one place the shape is proved against the real
    // `PlatformManagementConfig`, the way `the_managed_desired_state_mode_needs_nothing_else_stated`
    // proves `DesiredStateConfig`'s managed mode without a shipped example
    // of it either.
    let config: PlatformManagementConfig = serde_json::from_value(serde_json::json!({
        "environment": "lucentroot",
        "publication": {
            "namespace": "platform-system",
            "interval_seconds": 30,
        },
    }))
    .expect("a stated publication section must parse");

    let publication = config
        .publication
        .expect("the section was stated, so it must be Some");
    assert_eq!(publication.namespace, "platform-system");
    assert_eq!(publication.interval_seconds, 30);
}

#[test]
fn an_unknown_key_inside_the_publication_section_is_refused_rather_than_ignored() {
    let error = serde_json::from_value::<PlatformManagementConfig>(serde_json::json!({
        "environment": "lucentroot",
        "publication": {
            "namespace": "platform-system",
            "typo_field": true,
        },
    }))
    .expect_err("a field the publisher does not model must not be silently dropped");

    assert!(error.to_string().contains("typo_field"), "{error}");
}
