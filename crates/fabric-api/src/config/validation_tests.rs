//! The cross-domain checks, and what each one is protecting against.

use std::path::PathBuf;

use fabric_connector_ndc::NdcConnectorConfig;
use fabric_core::TenantId;
use fabric_identity::{IdentityConfig, TrustedIssuer};

use crate::config::{Allowlist, AppConfig, TokenConfig};

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

// -------------------------------------------- the two issuer lists (ADR 0019)

/// An identity registry binding each of `tenants` to its own realm issuer.
fn bound(tenants: &[&str]) -> IdentityConfig {
    IdentityConfig {
        trusted_issuers: tenants
            .iter()
            .map(|tenant| TrustedIssuer::new(issuer(tenant), TenantId::try_new(tenant).unwrap()))
            .collect(),
        ..IdentityConfig::default()
    }
}

fn issuer(tenant: &str) -> String {
    format!("https://identity.fabric.example/realms/{tenant}")
}

/// A defence-in-depth posture verifying signatures for exactly `tenants`.
fn validating(tenants: &[&str]) -> TokenConfig {
    TokenConfig::Validating {
        jwks_path: PathBuf::from("/etc/fabric/jwks.json"),
        issuers: Some(Allowlist::try_new(tenants.iter().map(|tenant| issuer(tenant)).collect()).unwrap()),
        audiences: None,
    }
}

#[test]
fn a_validating_deployment_must_name_the_same_issuers_twice() {
    // An issuer whose signature is accepted but which binds no tenant is a
    // token that verifies and cannot be placed; the reverse is a tenant
    // binding for an issuer nobody will accept. Both are startup errors.
    let config = AppConfig {
        token: validating(&["acme", "globex"]),
        identity: bound(&["acme", "initech"]),
        ..config_with(vec![connector("postgres")])
    };

    let error = config.validate().unwrap_err();

    assert!(error.contains("[token].issuers"));
    assert!(error.contains("[identity].trusted_issuers"));
    assert!(error.contains(&issuer("globex")));
    assert!(error.contains(&issuer("initech")));
}

#[test]
fn a_validating_deployment_naming_one_set_twice_is_accepted() {
    let config = AppConfig {
        token: validating(&["acme", "globex"]),
        identity: bound(&["globex", "acme"]),
        ..config_with(vec![connector("postgres")])
    };

    assert!(config.validate().is_ok(), "order is not part of the set");
}

#[test]
fn the_canonical_posture_has_no_second_issuer_list_to_diverge_from() {
    // The edge holds the allowlist in this posture, so there is nothing here
    // to compare against and this check must not invent a requirement.
    let config = AppConfig {
        token: TokenConfig::TrustedIngress {},
        identity: bound(&["acme"]),
        ..config_with(vec![connector("postgres")])
    };

    assert!(config.validate().is_ok());
}

#[test]
fn an_unexamined_iss_under_the_validating_posture_is_still_a_valid_deployment() {
    // Omitting `[token].issuers` means "do not examine `iss` when verifying
    // the signature". The tenant binding still refuses an unregistered issuer,
    // so this fails closed and is not this check's business.
    let config = AppConfig {
        token: TokenConfig::Validating {
            jwks_path: PathBuf::from("/etc/fabric/jwks.json"),
            issuers: None,
            audiences: None,
        },
        identity: bound(&["acme"]),
        ..config_with(vec![connector("postgres")])
    };

    assert!(config.validate().is_ok());
}

#[test]
fn an_empty_issuer_registry_is_not_this_checks_business() {
    // It is `IdentityConfig::validate`'s, reached from `build_identity` — so
    // `AppConfig::validate` passing is not evidence the process will start.
    // `composed_surface.rs` is what proves that.
    let config = AppConfig {
        identity: IdentityConfig::default(),
        ..config_with(vec![connector("postgres")])
    };

    assert!(config.validate().is_ok());
    assert!(config.identity.validate().is_err());
}

#[test]
fn validate_runs_the_administrator_role_check() {
    // The wiring, not the rule: `administrator_role` has its own tests for
    // what it rejects, and this pins that `validate` actually calls it.
    let mut config = config_with(vec![connector("postgres")]);
    config.permissions.administrator_role = String::new();

    assert!(config.validate().is_err());
}
